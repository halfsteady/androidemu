// Host-owned virtual device. Dolphin still evaluates its normal controller expressions,
// including motion simulation; SDL supplies normalized input instead of OS key polling.
#include <algorithm>
#include <cmath>
#include <memory>
#include <vector>
#include "Common/IniFile.h"
#include "Core/Config/MainSettings.h"
#include "Core/HW/GCPad.h"
#include "Core/HW/Wiimote.h"
#include "Core/HW/SI/SI_Device.h"
#include "InputCommon/InputConfig.h"
#include "InputCommon/ControllerEmu/ControllerEmu.h"
#include "InputCommon/ControllerEmu/ControlGroup/Attachments.h"
#include "InputCommon/ControllerInterface/ControllerInterface.h"

class HostInput final : public ciface::Core::Device::Input {
public:
  explicit HostInput(std::string name) : m_name(std::move(name)) {}
  std::string GetName() const override { return m_name; }
  ControlState GetState() const override { return value.load(); }
  std::atomic<double> value{0};
private:
  std::string m_name;
};
class HostDevice final : public ciface::Core::Device {
public:
  std::string GetName() const override { return "Controls"; }
  std::string GetSource() const override { return "Emulia"; }
  bool IsVirtualDevice() const override { return true; }
  void Add(HostInput* input) { AddInput(input); }
};
static std::shared_ptr<HostDevice> s_device;
static std::vector<HostInput*> s_inputs;
static std::vector<ControlReference*> s_references;
static std::string s_schema;
static Common::IniFile::Section s_gc_config, s_wii_config;

static void BindGroups(ControllerEmu::ControlGroupContainer* controller,
                       Common::IniFile::Section* section, const std::string& prefix,
                       const std::string& base = "") {
  for (const auto& group : controller->groups) {
    if (group->type == ControllerEmu::GroupType::Attachments) {
      auto* attachments = static_cast<ControllerEmu::Attachments*>(group.get());
      for (const auto& attachment : attachments->GetAttachmentList()) {
        const auto name = attachment->GetName();
        if (name == "Nunchuk" || name == "Classic")
          BindGroups(attachment.get(), section, name, name + "/");
      }
    } else if (group->name != "Rumble") {
      for (const auto& control : group->controls) {
        const auto path = group->name + "/" + control->name;
        auto* input = new HostInput(prefix + "/" + path);
        s_device->Add(input);
        s_inputs.push_back(input);
        s_references.push_back(control->control_ref.get());
        s_schema += input->GetName() + "\n";
        section->Set(base + path, "`" + input->GetName() + "`");
      }
    }
  }
}
static void InitHostControls(int style) {
  s_inputs.clear(); s_references.clear(); s_schema.clear();
  s_device = std::make_shared<HostDevice>();
  auto* gc = Pad::GetConfig()->GetController(0);
  auto* wii = Wiimote::GetConfig()->GetController(0);
  s_gc_config = {}; s_wii_config = {};
  gc->SaveConfig(&s_gc_config); wii->SaveConfig(&s_wii_config);
  BindGroups(gc, &s_gc_config, "GC");
  BindGroups(wii, &s_wii_config, "Wii");
  g_controller_interface.AddDevice(s_device);
  s_gc_config.Set("Device", s_device->GetQualifiedName());
  s_wii_config.Set("Device", s_device->GetQualifiedName());
  s_wii_config.Set("Extension", std::string(style == 2 ? "Nunchuk" : style == 3 ? "Classic" : "None"));
  s_wii_config.Set("Options/Sideways Wiimote", style == 1);
  gc->LoadConfig(&s_gc_config); wii->LoadConfig(&s_wii_config);
  gc->UpdateReferences(g_controller_interface); wii->UpdateReferences(g_controller_interface);
  // Core reloads controller configuration on its boot thread.
  Pad::GetConfig()->SaveConfig();
  Wiimote::GetConfig()->SaveConfig();
  Config::SetBase(Config::GetInfoForSIDevice(0), style == 0 ? SerialInterface::SIDEVICE_GC_CONTROLLER : SerialInterface::SIDEVICE_NONE);
}
extern "C" const char* emulia_dolphin_controls() { return s_schema.c_str(); }
extern "C" void emulia_dolphin_input(unsigned int index, double value) {
  if (index < s_inputs.size()) s_inputs[index]->value = std::isfinite(value) ? std::clamp(value, 0.0, 1.0) : 0.0;
}

#include "InputCommon/GCPadStatus.h"
extern "C" unsigned long long emulia_dolphin_pad_diagnostics() {
  const auto pad = Pad::GetStatus(0);
  return static_cast<unsigned long long>(pad.button) |
         (static_cast<unsigned long long>(pad.stickX) << 16) |
         (static_cast<unsigned long long>(pad.stickY) << 24) |
         (static_cast<unsigned long long>(pad.triggerLeft) << 32) |
         (static_cast<unsigned long long>(pad.triggerRight) << 40);
}
extern "C" unsigned int emulia_dolphin_style_diagnostics() {
  Common::IniFile::Section section;
  Wiimote::GetConfig()->GetController(0)->SaveConfig(&section);
  std::string extension; bool sideways = false;
  section.Get("Extension", &extension);
  section.Get("Options/Sideways Wiimote", &sideways);
  return (extension == "Classic" ? 3 : extension == "Nunchuk" ? 2 : sideways ? 1 : 0) |
         (Config::Get(Config::GetInfoForSIDevice(0)) == SerialInterface::SIDEVICE_GC_CONTROLLER ? 256 : 0);
}

extern "C" double emulia_dolphin_control_diagnostics(unsigned int index) {
  const auto lock = ControllerEmu::EmulatedController::GetStateLock();
  return index < s_references.size() ? s_references[index]->GetState<double>() : 0.0;
}
