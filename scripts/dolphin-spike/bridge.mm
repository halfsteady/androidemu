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

static NSView* s_view;
static std::atomic<bool> s_stop{false};
static bool s_initialized = false;
static std::atomic<bool> s_focused{true};
static NSSize s_size;
std::vector<std::string> Host_GetPreferredLocales() { return {}; }
bool Host_UIBlocksControllerState() { return false; }
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
    Core::Shutdown(Core::System::GetInstance());
    UICommon::ShutdownControllers();
    UICommon::Shutdown();
    s_initialized = false;
  }
  [s_view removeFromSuperview];
  s_view = nil;
}
extern "C" int emulia_dolphin_start(void* window, const char* path, const char* user) {
  @autoreleasepool {
    if (s_initialized || !window || !path || !user) return 0;
    s_stop = false;
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
