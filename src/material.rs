use ash::vk;
use eyre::Ok;
use glam::Vec4;

use crate::{
    descriptors::{DescriptorAllocator, DescriptorLayoutBuilder, DescriptorWriter},
    graphics::{Blending, GraphicsPipelineInfo},
    mesh::GPUDrawPushConstants,
    shader::ShaderCompiler,
    texture::{AllocatedImage, DrawImage},
};

pub struct MaterialPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl MaterialPipeline {
    fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.destroy_pipeline_layout(self.layout, None) };
        unsafe { device.destroy_pipeline(self.pipeline, None) };
    }
}

pub struct MaterialInstance {
    pipeline_handle: MaterialHandle,
    set: vk::DescriptorSet,
    pass: MaterialPass,
}

#[derive(Clone, Copy)]
pub enum MaterialPass {
    MainColor,
    Transparent,
    Other,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)] //256 alignment
pub struct MaterialConstants {
    color_factors: Vec4,
    metal_color_factors: Vec4,
    _pad: [Vec4; 14],
}

impl MaterialConstants {
    pub fn new(color_factors: Vec4, metal_color_factors: Vec4) -> Self {
        Self {
            color_factors,
            metal_color_factors,
            _pad: [Vec4::ZERO; 14],
        }
    }
}

pub struct MaterialResources {
    pub color_image_view: vk::ImageView,
    pub color_sampler: vk::Sampler,
    pub metal_rough_image_vew: vk::ImageView,
    pub metal_rough_sampler: vk::Sampler,
    pub data_buffer: vk::Buffer,
    pub data_buffer_offset: u64,
}

pub struct GLTFMetallicRoughness {
    opaque_handle: MaterialHandle,
    transparent_handle: MaterialHandle,
    pipeline_layout: vk::PipelineLayout,
    material_map: MaterialMap,
    material_layout: vk::DescriptorSetLayout,
}
slotmap::new_key_type! { struct MaterialHandle; }
type MaterialMap = slotmap::SlotMap<MaterialHandle, MaterialPipeline>;
impl GLTFMetallicRoughness {
    pub fn new(
        device: &ash::Device,
        shader_compiler: &ShaderCompiler,
        scene_data_layout: vk::DescriptorSetLayout,
        draw_image: &DrawImage,
        depth_image: &AllocatedImage,
    ) -> eyre::Result<Self> {
        let shader_src = include_str!("../shaders/mesh.vert");
        let vert_shader = shader_compiler.create_shader_module_from_str(
            device,
            shader_src,
            shaderc::ShaderKind::Vertex,
            "mesh.vert",
            "main",
        )?;
        let shader_src = include_str!("../shaders/mesh.frag");
        let frag_shader = shader_compiler.create_shader_module_from_str(
            device,
            shader_src,
            shaderc::ShaderKind::Fragment,
            "mesh.frag",
            "main",
        )?;
        let push_constants_range = vk::PushConstantRange::default()
            .offset(0)
            .size(std::mem::size_of::<GPUDrawPushConstants>() as u32)
            .stage_flags(vk::ShaderStageFlags::VERTEX);
        let material_layout = DescriptorLayoutBuilder::new()
            .add_binding(0, vk::DescriptorType::UNIFORM_BUFFER)
            .add_binding(1, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .add_binding(2, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .build(
                device,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )?;
        let layouts = [scene_data_layout, material_layout];
        let ranges = [push_constants_range];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(&ranges);

        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;
        let pipeline = GraphicsPipelineInfo::builder()
            .shaders([vert_shader, frag_shader])
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_enabled(true)
            .color_attachment_format(draw_image.format())
            .depth_format(depth_image.format())
            .layout(pipeline_layout)
            .build()
            .create(device)?;

        let mut material_map = MaterialMap::with_key();
        let opaque_pipeline = MaterialPipeline {
            pipeline,
            layout: pipeline_layout,
        };
        let opaque_handle = material_map.insert(opaque_pipeline);

        let pipeline = GraphicsPipelineInfo::builder()
            .shaders([vert_shader, frag_shader])
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_enabled(false)
            .color_attachment_format(draw_image.format())
            .depth_format(depth_image.format())
            .layout(pipeline_layout)
            .blending(Blending::Additive)
            .build()
            .create(device)?;
        let transparent_pipeline = MaterialPipeline {
            pipeline,
            layout: pipeline_layout,
        };
        let transparent_handle = material_map.insert(transparent_pipeline);

        unsafe { device.destroy_shader_module(vert_shader, None) };
        unsafe { device.destroy_shader_module(frag_shader, None) };
        Ok(Self {
            opaque_handle,
            transparent_handle,
            material_map,
            material_layout,
            pipeline_layout,
        })
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        if let Some(m) = self.material_map.remove(self.opaque_handle) {
            unsafe { device.destroy_pipeline(m.pipeline, None) };
        }
        if let Some(m) = self.material_map.remove(self.transparent_handle) {
            unsafe { device.destroy_pipeline(m.pipeline, None) };
        }
        unsafe { device.destroy_descriptor_set_layout(self.material_layout, None) };
        unsafe { device.destroy_pipeline_layout(self.pipeline_layout, None) };
    }
    pub fn write_material(
        &self,
        device: &ash::Device,
        pass: MaterialPass,
        resources: &MaterialResources,
        descriptor_allocator: &DescriptorAllocator,
    ) -> eyre::Result<MaterialInstance> {
        let pipeline_handle = match pass {
            MaterialPass::MainColor | MaterialPass::Other => self.opaque_handle,
            MaterialPass::Transparent => self.transparent_handle,
        };
        let set = descriptor_allocator.allocate(device, self.material_layout)?[0];
        DescriptorWriter::new()
            .write_buffer(
                0,
                resources.data_buffer,
                resources.data_buffer_offset,
                std::mem::size_of::<MaterialConstants>() as u64,
                vk::DescriptorType::UNIFORM_BUFFER,
            )
            .write_image(
                1,
                resources.color_image_view,
                resources.color_sampler,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            )
            .write_image(
                2,
                resources.metal_rough_image_vew,
                resources.metal_rough_sampler,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            )
            .update_set(device, set);

        Ok(MaterialInstance {
            pipeline_handle,
            set,
            pass,
        })
    }
}
