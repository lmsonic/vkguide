#version 450
#extension GL_GOOGLE_include_directive : require
#include "input_structs.glsl"

layout(location = 0) in vec3 in_normal;
layout(location = 1) in vec3 in_color;
layout(location = 2) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

void main() {

  float light_value = max(dot(in_normal, scene_data.sun_direction.xyz), 0.0);
  vec3 color = in_color * texture(color_texture, in_uv).xyz;
  vec3 ambient = in_color * scene_data.ambient_color.xyz;

  out_color = vec4(color * light_value * scene_data.sun_color.w + ambient, 1.0);
}