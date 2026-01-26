use std::sync::Arc;

use ash::vk;
use glam::Vec4;

use crate::{
    descriptors::{DescriptorAllocator, DescriptorLayoutBuilder},
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
    pipeline: MaterialPipeline,
    set: vk::DescriptorSet,
    pass: MaterialPass,
}

#[derive(Clone, Copy)]
enum MaterialPass {
    MainColor,
    Transparent,
    Other,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)] //256 alignment
struct MaterialConstants {
    color_factors: Vec4,
    metal_color_factors: Vec4,
    _pad: [Vec4; 14],
}

pub struct MaterialResources {
    color_image: AllocatedImage,
    color_sampler: vk::Sampler,
    metal_rough_image: AllocatedImage,
    metal_rough_sampler: vk::Sampler,
    data_buffer: vk::Buffer,
    data_buffer_offset: u32,
}

pub struct GLTFMetallicRoughness {
    opaque_pipeline: MaterialPipeline,
    transparent_pipeline: MaterialPipeline,
    material_layout: vk::DescriptorSetLayout,
}

impl GLTFMetallicRoughness {
    fn new(
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

        let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;
        let pipeline = GraphicsPipelineInfo::builder()
            .shaders([vert_shader, frag_shader])
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_enabled(true)
            .color_attachment_format(draw_image.format())
            .depth_format(depth_image.format())
            .layout(layout)
            .build()
            .create(device)?;

        let opaque_pipeline = MaterialPipeline { pipeline, layout };
        let pipeline = GraphicsPipelineInfo::builder()
            .shaders([vert_shader, frag_shader])
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_enabled(false)
            .color_attachment_format(draw_image.format())
            .depth_format(depth_image.format())
            .layout(layout)
            .blending(Blending::Additive)
            .build()
            .create(device)?;
        let transparent_pipeline = MaterialPipeline { pipeline, layout };

        unsafe { device.destroy_shader_module(vert_shader, None) };
        unsafe { device.destroy_shader_module(frag_shader, None) };
        Ok(Self {
            opaque_pipeline: opaque_pipeline.into(),
            transparent_pipeline: transparent_pipeline.into(),
            material_layout,
        })
    }
    fn destroy(&mut self, device: &ash::Device) {
        self.opaque_pipeline.destroy(device);
        self.transparent_pipeline.destroy(device);
        unsafe { device.destroy_descriptor_set_layout(self.material_layout, None) };
    }
    fn write_material(
        &mut self,
        device: &ash::Device,
        pass: MaterialPass,
        resources: &MaterialResources,
        descriptor_allocator: &mut DescriptorAllocator,
    ) {
    }
}
