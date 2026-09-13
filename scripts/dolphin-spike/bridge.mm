// SPDX-License-Identifier: GPL-3.0-only
// Experimental host for Dolphin 2606a. All entry points run on Emulia's UI thread.
#include <AppKit/AppKit.h>
#include <atomic>
#include <cstdio>
#include "Common/FileUtil.h"
#include "Common/MsgHandler.h"
#include "Common/WindowSystemInfo.h"
#include "Core/Boot/Boot.h"
#include "Core/BootManager.h"
#include "Core/Core.h"
#include "Core/Host.h"
#include "Core/System.h"
#include "UICommon/UICommon.h"
#include "VideoCommon/Present.h"
#include "VideoCommon/FrameDumper.h"
#include "Core/State.h"
#include "Core/HW/ProcessorInterface.h"
#include "controls.inc"
#include "pointer_geometry.h"

static NSView* s_view;
static std::atomic<bool> s_stop{false};
static bool s_initialized = false;
static std::atomic<bool> s_focused{true};
static NSSize s_size;
static std::atomic<bool> s_paused{false};
std::vector<std::string> Host_GetPreferredLocales() { return {}; }
bool Host_UIBlocksControllerState() { return s_paused || !s_focused; }
bool Host_RendererHasFocus() { return s_focused; }
bool Host_RendererHasFullFocus() { return Host_RendererHasFocus(); }
bool Host_RendererIsFullscreen() { return false; }
bool Host_TASInputHasFocus() { return false; }
void Host_Message(HostMessageID id) { if (id == HostMessageID::WMUserStop) s_stop = true; }
void Host_PPCSymbolsChanged() {}
void Host_PPCBreakpointsChanged() {}
void Host_RequestRenderWindowSize(int, int) {}
void Host_UpdateDisasmDialog() {}
void Host_JitCacheInvalidation() {}
void Host_JitProfileDataWiped() {}
void Host_UpdateTitle(const std::string& title) { fprintf(stderr, "Dolphin probe: %s\n", title.c_str()); }
void Host_YieldToUI() {}
void Host_TitleChanged() {}
void Host_UpdateDiscordClientID(const std::string&) {}
bool Host_UpdateDiscordPresenceRaw(const std::string&, const std::string&, const std::string&,
 const std::string&, const std::string&, const std::string&, int64_t, int64_t, int, int) { return false; }
std::unique_ptr<GBAHostInterface> Host_CreateGBAHost(std::weak_ptr<HW::GBA::Core>) { return nullptr; }

extern "C" void emulia_dolphin_stop() {
  if (s_initialized) {
    Core::Stop(Core::System::GetInstance());
    // Quartz can synchronously dispatch to AppKit during boot/teardown. Keep
    // the main queue alive until the emulation thread has actually stopped.
    while (Core::GetState(Core::System::GetInstance()) != Core::State::Uninitialized) {
      Core::HostDispatchJobs(Core::System::GetInstance());
      [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                              beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
    }
    Core::Shutdown(Core::System::GetInstance());
    UICommon::ShutdownControllers();
    s_inputs.clear(); s_references.clear(); s_device.reset();
    UICommon::Shutdown();
    s_initialized = false;
  }
  [s_view removeFromSuperview];
  s_view = nil;
}
extern "C" int emulia_dolphin_start(void* window, const char* path, const char* user, int style) {
  @autoreleasepool {
    if (s_initialized || !window || !path || !user) return 0;
    s_stop = false; s_paused = false;
    NSView* parent = [(__bridge NSWindow*)window contentView];
    NSRect frame = parent.bounds;
    frame.origin.y = 84;
    frame.size.height = MAX(1, frame.size.height - 136);
    s_view = [[NSView alloc] initWithFrame:frame];
    s_size = frame.size;
    s_view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [parent addSubview:s_view];
    if (!File::IsDirectory(File::GetSysDirectory())) { [s_view removeFromSuperview]; s_view = nil; return 0; }
    UICommon::SetUserDirectory(user);
    Common::RegisterMsgAlertHandler([](const char* caption, const char* text, bool, Common::MsgType) {
      fprintf(stderr, "Dolphin probe alert: %s: %s\n", caption, text);
      return false; // Never silently accept an unexpected warning/question.
    });
    UICommon::Init();
    WindowSystemInfo wsi(WindowSystemType::MacOS, nullptr, (__bridge void*)s_view, (__bridge void*)s_view);
    wsi.render_surface_scale = parent.window.backingScaleFactor;
    UICommon::InitControllers(wsi);
    InitHostControls(style);
    s_initialized = true;
    auto boot = BootParameters::GenerateFromFile(path);
    if (!boot || !BootManager::BootCore(Core::System::GetInstance(), std::move(boot), wsi)) {
      emulia_dolphin_stop();
      return 0;
    }
    return 1;
  }
}
extern "C" int emulia_dolphin_pump() {
  if (!s_initialized) return 0;
  Core::HostDispatchJobs(Core::System::GetInstance());
  s_focused = [s_view.window isKeyWindow];
  if (!NSEqualSizes(s_size, s_view.bounds.size)) {
    s_size = s_view.bounds.size;
    if (g_presenter) g_presenter->ResizeSurface();
  }
  return !s_stop && Core::GetState(Core::System::GetInstance()) != Core::State::Uninitialized;
}

extern "C" int emulia_dolphin_abi() { return 3; }
extern "C" int emulia_dolphin_pause(int paused) {
  auto& system = Core::System::GetInstance();
  const auto state = Core::GetState(system);
  if (state != Core::State::Running && state != Core::State::Paused) return 0;
  s_paused = paused != 0;
  Core::SetState(system, s_paused ? Core::State::Paused : Core::State::Running);
  s_view.hidden = s_paused;
  return 1;
}
extern "C" int emulia_dolphin_screenshot(const char* path) {
  if (!g_frame_dumper || !path) return 0;
  const Core::CPUThreadGuard guard(Core::System::GetInstance());
  g_frame_dumper->SaveScreenshot(path);
  return 1;
}
extern "C" int emulia_dolphin_state(const char* path, int load) {
  auto& system = Core::System::GetInstance();
  if (!path || Core::GetState(system) != Core::State::Paused) return 0;
  const Core::CPUThreadGuard guard(system);
  return load ? State::LoadAsChecked(system, path) : State::SaveAsChecked(system, path);
}
// Read the pointer relative to Dolphin's actual child view, including after
// resize/fullscreen. This is the same AppKit input source used by Quartz.
extern "C" unsigned int emulia_dolphin_pointer(double* xy) {
  if (!xy || !s_view || ![s_view.window isKeyWindow] || s_view.hidden) return 0;
  const NSPoint point = [s_view convertPoint:[s_view.window mouseLocationOutsideOfEventStream] fromView:nil];
  const NSRect bounds = s_view.bounds;
  if (bounds.size.width <= 0 || bounds.size.height <= 0 || !NSPointInRect(point, bounds)) return 0;
  // Match Quartz::KeyboardAndMouse: the presenter updates this atomic scale
  // when the aspect ratio or surface changes. Whole-view normalization alone
  // is correct at the center but drifts toward letterboxed/pillarboxed edges.
  const auto scale = g_controller_interface.GetWindowInputScale();
  const auto position = GamePointer(point.x - bounds.origin.x, point.y - bounds.origin.y,
                                    bounds.size.width, bounds.size.height, scale.x, scale.y);
  if (!position) return 0;
  xy[0] = (*position)[0];
  xy[1] = (*position)[1];
  return 0x80000000u | (static_cast<unsigned int>([NSEvent pressedMouseButtons]) & 3u);
}
