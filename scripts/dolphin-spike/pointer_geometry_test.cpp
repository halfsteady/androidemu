// SPDX-License-Identifier: GPL-3.0-only
#include "pointer_geometry.h"
#include <cassert>

static void Near(double actual, double expected) { assert(std::abs(actual - expected) < 1e-9); }
int main() {
  // 16:9 picture in a 1000x750 view: image is 1000x562.5, centered vertically.
  auto point = GamePointer(500, 375, 1000, 750, 1, 4.0 / 3);
  assert(point); Near((*point)[0], 0); Near((*point)[1], 0);
  point = GamePointer(750, 515.625, 1000, 750, 1, 4.0 / 3);
  assert(point); Near((*point)[0], .5); Near((*point)[1], .5);
  assert(!GamePointer(500, 50, 1000, 750, 1, 4.0 / 3)); // top/bottom bars
  // 4:3 picture in a 1600x900 view: 1200px wide, with 200px side bars.
  point = GamePointer(1100, 225, 1600, 900, 4.0 / 3, 1);
  assert(point); Near((*point)[0], .5); Near((*point)[1], -.5);
  assert(!GamePointer(100, 450, 1600, 900, 4.0 / 3, 1));
  // Retina: scaling every coordinate equally must not change the result.
  auto retina = GamePointer(2200, 450, 3200, 1800, 4.0 / 3, 1);
  assert(retina); Near((*retina)[0], (*point)[0]); Near((*retina)[1], (*point)[1]);
  assert(!GamePointer(0, 0, 0, 0, 1, 1));
}
