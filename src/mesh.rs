use ash::vk;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::{
    buffer::AllocatedBuffer, immediate::ImmediateSubmit, utils::memcopy, vulkan::QueueFamilyIndices,
};

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    pos: Vec3,
    uv_x: f32,
    normal: Vec3,
    uv_y: f32,
    color: Vec4,
}

impl Vertex {
    pub const fn new(pos: Vec3, color: Vec4) -> Self {
        let uv = Vec2::ZERO;
        let normal = Vec3::ZERO;
        Self {
            pos,
            uv_x: uv.x,
            normal,
            uv_y: uv.y,
            color,
        }
    }
}
pub struct GPUMeshBuffers {
    index_buffer: AllocatedBuffer,
    vertex_buffer: AllocatedBuffer,
    vertex_buffer_addr: vk::DeviceAddress,
}

impl GPUMeshBuffers {
    pub fn new(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        transfer_queue: vk::Queue,
        immediate_submit: &ImmediateSubmit,
        indices: &[u32],
        vertices: &[Vertex],
    ) -> eyre::Result<Self> {
        let vertex_buffer_size = std::mem::size_of_val(vertices);
        let vertex_buffer = AllocatedBuffer::new(
            allocator,
            vertex_buffer_size as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk_mem::MemoryUsage::AutoPreferDevice,
        )?;

        let index_buffer_size = std::mem::size_of_val(indices);
        let index_buffer = AllocatedBuffer::new(
            allocator,
            index_buffer_size as u64,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk_mem::MemoryUsage::AutoPreferDevice,
        )?;

        let device_addr_info =
            vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer.buffer());
        let vertex_buffer_addr = unsafe { device.get_buffer_device_address(&device_addr_info) };

        // Write data

        let mut staging = AllocatedBuffer::new(
            allocator,
            (vertex_buffer_size + index_buffer_size) as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk_mem::MemoryUsage::AutoPreferHost,
        )?;
        let memory = unsafe { allocator.map_memory(&mut staging.allocation()) }?;

        unsafe { memcopy(vertices, memory) };
        let memory_indices = memory.wrapping_byte_add(vertex_buffer_size);
        unsafe { memcopy(indices, memory_indices) };

        immediate_submit.submit(device, transfer_queue, |cmd| {
            let vertex_copy = vk::BufferCopy::default().size(vertex_buffer_size as u64);
            unsafe {
                device.cmd_copy_buffer(
                    cmd,
                    staging.buffer(),
                    vertex_buffer.buffer(),
                    &[vertex_copy],
                );
            };
            let index_copy = vk::BufferCopy::default()
                .src_offset(vertex_buffer_size as u64)
                .size(index_buffer_size as u64);
            unsafe {
                device.cmd_copy_buffer(cmd, staging.buffer(), index_buffer.buffer(), &[index_copy]);
            };
        })?;

        unsafe { allocator.unmap_memory(&mut staging.allocation()) };
        staging.destroy(allocator);

        Ok(Self {
            index_buffer,
            vertex_buffer,
            vertex_buffer_addr,
        })
    }
    pub fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        self.index_buffer.destroy(allocator);
        self.vertex_buffer.destroy(allocator);
    }

    pub const fn vertex_buffer_addr(&self) -> u64 {
        self.vertex_buffer_addr
    }

    pub const fn vertex_buffer(&self) -> &AllocatedBuffer {
        &self.vertex_buffer
    }

    pub const fn index_buffer(&self) -> &AllocatedBuffer {
        &self.index_buffer
    }
}
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, align(4))]
pub struct GPUDrawPushConstants {
    world_matrix: Mat4,
    vertex_buffer_addr: vk::DeviceAddress,
    _pad: Vec2,
}

impl GPUDrawPushConstants {
    pub const fn new(world_matrix: Mat4, vertex_buffer_addr: vk::DeviceAddress) -> Self {
        Self {
            world_matrix,
            vertex_buffer_addr,
            _pad: Vec2::ZERO,
        }
    }
}
