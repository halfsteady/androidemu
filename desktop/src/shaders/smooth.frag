#version 330 core
// scale2x, run once per step. Where a single pixel's two neighbours
// towards one of its corners agree with each other and the opposite pair
// disagrees, that corner is a stairstep rather than a drawn corner, so it
// gets cut away. Everything else is left exactly as the console drew it,
// which is what keeps flat colour flat.
//
// Addressing is by fragment coordinate and texelFetch throughout: the
// comparisons are equality tests, and a filtered or half-texel-off sample
// would quietly make them all false.
uniform sampler2D src; uniform ivec2 srcSize;
out vec4 color;
vec3 at(ivec2 p){ return texelFetch(src, clamp(p, ivec2(0), srcSize - 1), 0).rgb; }
void main(){
  ivec2 o = ivec2(gl_FragCoord.xy);
  ivec2 c = o / 2;          // the source texel this output sits in
  ivec2 q = o - c * 2;      // and which of its four quarters
  vec3 e = at(c);
  vec3 up = at(c + ivec2(0, -1)), down = at(c + ivec2(0, 1));
  vec3 left = at(c + ivec2(-1, 0)), right = at(c + ivec2(1, 0));
  vec3 result = e;
  // Inside a solid run both pairs match and nothing is cut.
  if (up != down && left != right) {
    vec3 v = q.y == 0 ? up : down;
    vec3 h = q.x == 0 ? left : right;
    if (v == h) result = v;
  }
  color = vec4(result, 1.0);
}
