use std::path::Path;

use ash::vk;
use eyre::{Context, OptionExt};
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::{buffer::AllocatedBuffer, immediate::ImmediateSubmit, utils::memcopy};

#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GPUSceneData {
    view: Mat4,
    proj: Mat4,
    view_proj: Mat4,
    ambient_color: Vec4,
    sun_direction: Vec4,
    sun_color: Vec4,
}

impl GPUSceneData {
    pub fn new(
        view: Mat4,
        proj: Mat4,
        ambient_color: Vec4,
        sun_direction: Vec4,
        sun_color: Vec4,
    ) -> Self {
        Self {
            view,
            proj,
            view_proj: view * proj,
            ambient_color,
            sun_direction,
            sun_color,
        }
    }
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
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

#[derive(Debug)]
pub struct GeoSurface {
    start_index: u32,
    count: u32,
}

impl GeoSurface {
    pub const fn start_index(&self) -> u32 {
        self.start_index
    }

    pub const fn count(&self) -> u32 {
        self.count
    }
}
pub struct Mesh {
    name: String,
    surfaces: Vec<GeoSurface>,
    mesh_buffers: GPUMeshBuffers,
}

pub fn load_gltf_from_path(
    path: impl AsRef<Path>,
    device: &ash::Device,
    allocator: &vk_mem::Allocator,
    transfer_queue: vk::Queue,
    transfer_immediate: &ImmediateSubmit,
) -> eyre::Result<Vec<Mesh>> {
    let (gltf, buffers, _) = gltf::import(path).wrap_err("could not open")?;
    let mut meshes = Vec::with_capacity(gltf.meshes().len());
    let mut indices = vec![];
    let mut vertices = vec![];

    for mesh in gltf.meshes() {
        let name = mesh.name().map_or_else(
            || format!("Mesh #{}", mesh.index()),
            std::string::ToString::to_string,
        );
        println!("loading mesh {name}");
        indices.clear();
        vertices.clear();
        let mut surfaces = Vec::with_capacity(mesh.primitives().len());
        for prim in mesh.primitives() {
            println!("loading primitive #{}", prim.index());
            let reader = prim.reader(|buffer| Some(&buffers[buffer.index()]));
            let prim_indices = reader.read_indices().ok_or_eyre("could not read indices")?;

            let prim_indices = prim_indices.into_u32();

            let geo_surface = GeoSurface {
                start_index: indices.len() as u32,
                count: prim_indices.len() as u32,
            };
            surfaces.push(geo_surface);

            let initial_vert = vertices.len();

            // load indices
            indices.reserve(prim_indices.len());
            for i in prim_indices {
                indices.push(initial_vert as u32 + i);
            }

            // load positions
            let positions = reader
                .read_positions()
                .ok_or_eyre("could not read positions")?;
            vertices.resize(positions.len(), Vertex::default());
            for (i, p) in positions.enumerate() {
                vertices[initial_vert + i].pos = Vec3::new(p[0], p[1], p[2]);
            }
            if let Some(normals) = reader.read_normals() {
                for (i, n) in normals.enumerate() {
                    vertices[initial_vert + i].normal = Vec3::new(n[0], n[1], n[2]);
                }
            }
            if let Some(uvs) = reader.read_tex_coords(0) {
                for (i, uv) in uvs.into_f32().enumerate() {
                    vertices[initial_vert + i].uv_x = uv[0];
                    vertices[initial_vert + i].uv_y = uv[1];
                }
            }

            if let Some(colors) = reader.read_colors(0) {
                for (i, c) in colors.into_rgba_f32().enumerate() {
                    vertices[initial_vert + i].color = Vec4::new(c[0], c[1], c[2], c[3]);
                }
            }
        }
        const OVERRIDE_COLOR: bool = true;
        if OVERRIDE_COLOR {
            for v in &mut vertices {
                v.color = v.normal.extend(1.0);
            }
        }
        let mesh_buffers = GPUMeshBuffers::new(
            device,
            allocator,
            transfer_queue,
            transfer_immediate,
            &indices,
            &vertices,
        )?;
        meshes.push(Mesh {
            name,
            surfaces,
            mesh_buffers,
        });
    }

    Ok(meshes)
}

impl Mesh {
    pub const fn mesh_buffers_mut(&mut self) -> &mut GPUMeshBuffers {
        &mut self.mesh_buffers
    }
    pub const fn mesh_buffers(&self) -> &GPUMeshBuffers {
        &self.mesh_buffers
    }

    pub fn surfaces(&self) -> &[GeoSurface] {
        &self.surfaces
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
