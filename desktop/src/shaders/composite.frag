#version 330 core
// Composite video, encoded and decoded in one pass.
//
// A 2C02 does not emit colour; it emits a phase. This does the same and
// then reads it back the way a television had to, which is why the two
// famous consequences fall out rather than being drawn on: a one-pixel
// dither averages into a solid colour because chroma is carried at a
// fraction of luma's bandwidth, and the dots crawl because the phase
// walks a third of a cycle every line and every frame.
uniform sampler2D src; uniform ivec2 srcSize; uniform float framePhase;
out vec4 color;
const float PI = 3.14159265;
const float SUBS = 4.0;
vec3 fetch(int px, int py){
  return texelFetch(src, ivec2(clamp(px, 0, srcSize.x - 1), clamp(py, 0, srcSize.y - 1)), 0).rgb;
}
vec3 toYiq(vec3 c){
  return vec3(dot(c, vec3(0.299, 0.587, 0.114)),
              dot(c, vec3(0.5959, -0.2746, -0.3213)),
              dot(c, vec3(0.2115, -0.5227, 0.3112)));
}
void main(){
  int ox = int(gl_FragCoord.x);
  int oy = int(gl_FragCoord.y);
  // A line and a frame each walk the phase a third of a cycle.
  float base = 2.0 * PI * (float(oy) / 3.0 + framePhase);
  float luma = 0.0;
  vec2 chroma = vec2(0.0);
  float weight = 0.0;
  // Twenty-four samples is exactly four subcarrier cycles, so the
  // demodulation closes on a whole number of turns.
  for (int k = -12; k <= 11; k++) {
    int s = ox + k;
    vec3 c = toYiq(fetch(int(floor(float(s) / SUBS)), oy));
    float ph = base + 2.0 * PI * (2.0 / 3.0) * (float(s) / SUBS);
    vec2 cs = vec2(cos(ph), sin(ph));
    float signal = c.x + c.y * cs.x + c.z * cs.y;
    // Luma over exactly one cycle: the carrier sums to zero and
    // leaves the brightness detail behind.
    if (k >= -3 && k <= 2) luma += signal;
    // Chroma over the wide tapered window. This is the bleed.
    float w = 0.5 + 0.5 * cos(PI * (float(k) + 0.5) / 12.0);
    chroma += w * signal * cs;
    weight += w;
  }
  float y = luma / 6.0;
  vec2 iq = 2.0 * chroma / weight;
  vec3 rgb = vec3(y + 0.956 * iq.x + 0.619 * iq.y,
                  y - 0.272 * iq.x - 0.647 * iq.y,
                  y - 1.106 * iq.x + 1.703 * iq.y);
  color = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
