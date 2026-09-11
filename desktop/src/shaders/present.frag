#version 330 core
// One shader, one branch per look. Everything works in "picture space" -
// x and y both running 0..1 across the visible area - so a filter behaves
// the same whether or not the overscan rows are trimmed away.
in vec2 tex; uniform sampler2D screen; uniform int kind;
uniform float cols; uniform float rows; uniform vec2 window;
out vec4 color;
const float PI = 3.14159265;
float luma(vec3 c){ return dot(c, vec3(0.299, 0.587, 0.114)); }
// Between the quad's texture coordinates and the visible picture.
vec2 toPicture(vec2 uv){ return vec2(uv.x, (uv.y - window.x) / window.y); }
vec2 toTexture(vec2 p){ return vec2(p.x, window.x + p.y * window.y); }
vec3 pick(vec2 p){ return texture(screen, toTexture(p)).rgb; }
void main(){
  vec2 p = toPicture(tex);
  float edge = 1.0;
  // Old TV bends the picture over a tube and darkens the corners.
  // Outside the glass is black, not a smeared edge pixel.
  if (kind == 2) {
    vec2 c = p * 2.0 - 1.0;
    c *= 1.0 + 0.055 * dot(c, c);
    if (abs(c.x) > 1.0 || abs(c.y) > 1.0) { color = vec4(0.0, 0.0, 0.0, 1.0); return; }
    p = c * 0.5 + 0.5;
    edge = 1.0 - 0.28 * dot(c, c);
  }
  vec3 rgb = pick(p);
  float band = sin(PI * p.y * rows);
  if (kind == 1) {
    rgb *= 0.55 + 0.45 * band * band;
  } else if (kind == 2) {
    rgb *= 0.62 + 0.38 * band * band;
    // An aperture grille is a property of the glass, not of the game,
    // so its stripes are counted in device pixels.
    int stripe = int(mod(gl_FragCoord.x, 3.0));
    vec3 grille = stripe == 0 ? vec3(1.0, 0.72, 0.72)
                : stripe == 1 ? vec3(0.72, 1.0, 0.72) : vec3(0.72, 0.72, 1.0);
    rgb *= grille * edge;
  } else if (kind == 3) {
    // A gap around every console pixel, both axes, like an LCD grid.
    vec2 cell = fract(vec2(p.x * cols, p.y * rows));
    vec2 gap = smoothstep(0.0, 0.18, cell) * smoothstep(0.0, 0.18, 1.0 - cell);
    rgb *= mix(0.42, 1.0, min(gap.x, gap.y));
    rgb = mix(rgb, vec3(luma(rgb)) * vec3(0.92, 0.98, 0.9), 0.25);
  } else if (kind == 4) {
    float l = luma(rgb);
    vec3 dark = vec3(0.059, 0.220, 0.059), mid = vec3(0.188, 0.384, 0.188);
    vec3 light = vec3(0.545, 0.675, 0.059), pale = vec3(0.608, 0.737, 0.059);
    rgb = l < 0.25 ? dark : l < 0.5 ? mid : l < 0.75 ? light : pale;
  } else if (kind == 6) {
    float l = luma(rgb);
    rgb = clamp(vec3(l * 1.07 + 0.06, l * 0.94 + 0.03, l * 0.72), 0.0, 1.0);
  } else if (kind == 7) {
    // Cheap bloom: the brightest neighbour bleeds in, then the colour
    // is pushed away from grey.
    vec2 texel = vec2(1.0 / cols, 1.0 / rows);
    vec3 glow = max(max(pick(p + vec2(texel.x, 0.0)), pick(p - vec2(texel.x, 0.0))),
                    max(pick(p + vec2(0.0, texel.y)), pick(p - vec2(0.0, texel.y))));
    rgb += 0.40 * glow * glow;
    float l = luma(rgb);
    rgb = clamp(vec3(l) + (rgb - vec3(l)) * 1.7, 0.0, 1.0);
  } else if (kind == 8) {
    // Cartoon. The picture has already been smoothed into curves off
    // screen, so what is left is the part that makes it drawn rather
    // than rendered: flatten the shading into bands, push the colour,
    // and lay an ink line along the edges.
    //
    // The ink is a dark tint of the colour it runs through rather than
    // black, which is what stops a bright sprite from looking like it
    // was cut out and pasted down.
    // Half a console pixel out, which on the 4× buffer is two texels.
    vec2 rad = vec2(0.5 / cols, 0.5 / rows);
    vec3 a = pick(p - vec2(0.0, rad.y)), b = pick(p + vec2(0.0, rad.y));
    vec3 c = pick(p - vec2(rad.x, 0.0)), d = pick(p + vec2(rad.x, 0.0));
    // Opposite pairs rather than centre-to-neighbour, so the line sits
    // on the edge instead of along one side of it.
    float e = max(length(a - b), length(c - d));
    float ink = min(0.86, 0.50 * smoothstep(0.24, 0.34, e) + 0.40 * smoothstep(0.50, 0.62, e));
    // No posterising step, deliberately. Cel shading normally has to
    // flatten a rendered image into bands, but a 2C02 frame arrives
    // flat already: four colours to a tile, chosen by hand. Banding it
    // again only walks those colours onto a quantiser's grid and loses
    // what the artist picked. All this wants is a little more paint.
    float f = luma(rgb);
    rgb = vec3(f) + (rgb - vec3(f)) * 1.20;
    rgb = (rgb - 0.5) * 1.06 + 0.5;
    rgb *= mix(1.0, 0.13, ink);
    // A small mip level is a blur somebody else already paid for.
    vec3 glow = textureLod(screen, toTexture(p), 4.0).rgb;
    rgb = clamp(rgb + 0.30 * glow * glow, 0.0, 1.0);
  }
  if (kind != 2) rgb *= edge;
  color = vec4(rgb, 1.0);
}
