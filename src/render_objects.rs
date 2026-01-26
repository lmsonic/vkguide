use std::sync::Arc;

use ash::vk;
use glam::Affine3A;

use crate::material::MaterialInstance;

pub struct RenderObject {
    index_count: u32,
    first_index: u32,
    index_buffer: vk::Buffer,
    material_instance: Arc<MaterialInstance>,
    transform: Affine3A,
    vertex_buffer_addr: vk::DeviceAddress,
}

pub struct RenderContext {
    objects: Vec<RenderObject>,
}
trait Renderable {
    fn draw(&mut self, parent_matrix: &Affine3A, render_context: &mut RenderContext) {}
}
