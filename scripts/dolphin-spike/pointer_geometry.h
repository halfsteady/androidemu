// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include <array>
#include <cmath>
#include <optional>

// AppKit coordinates and dimensions must use the same units (Cocoa points).
// Dolphin's input scale accounts for the renderer's letterbox/pillarbox region.
inline std::optional<std::array<double, 2>> GamePointer(
    double x, double y, double width, double height, double scale_x, double scale_y) {
  if (!(width > 0 && height > 0 && scale_x > 0 && scale_y > 0)) return std::nullopt;
  const double nx = (2 * x / width - 1) * scale_x;
  const double ny = (2 * y / height - 1) * scale_y;
  if (!std::isfinite(nx) || !std::isfinite(ny) || std::abs(nx) > 1 || std::abs(ny) > 1)
    return std::nullopt;
  return std::array<double, 2>{nx, ny};
}
