#version 450
#extension GL_GOOGLE_include_directive : require
#extension GL_EXT_buffer_reference : require
#include "input_structs.glsl"

layout(location = 0) out vec3 out_normal;
layout(location = 1) out vec3 out_color;
layout(location = 2) out vec2 out_uv;

struct Vertex {
  vec3 pos;
  float uv_x;
  vec3 normal;
  float uv_y;
  vec4 color;
};

layout(buffer_reference, std430) readonly buffer VertexBuffer {
  Vertex vertices[];
};
layout(push_constant) uniform PushConstants {
  mat4 render_matrix;
  VertexBuffer vertex_buffer;
  vec2 pad;
}
push_constants;

void main() {
  Vertex v = push_constants.vertex_buffer.vertices[gl_VertexIndex];

  vec4 pos = vec4(v.pos, 1.0);
  gl_Position = scene_data.view_proj * push_constants.render_matrix * pos;

  out_normal = (push_constants.render_matrix * vec4(v.normal, 0.0)).xyz;
  out_color = v.color.xyz * material_data.color_factors.xyz;
  out_uv = vec2(v.uv_x, v.uv_y);
}