layout(set = 0, binding = 0) uniform SceneData {
  mat4 view;
  mat4 proj;
  mat4 view_proj;
  vec4 ambient_color;
  vec4 sun_direction;
  vec4 sun_color;
}
scene_data;

layout(set = 1, binding = 0) uniform GLTFMaterialData {
  vec4 color_factors;
  vec4 metal_rough_factors;
}
material_data;
layout(set = 1, binding = 1) uniform sampler2D color_texture;
layout(set = 1, binding = 2) uniform sampler2D metal_rough_texture;