use ash::vk;
use eyre::eyre;
use glam::Vec4;

use crate::{shader::ShaderCompiler, texture::DrawImage};
const RED: Vec4 = Vec4::new(1.0, 0.0, 0.0, 1.0);
const BLUE: Vec4 = Vec4::new(0.0, 0.0, 1.0, 1.0);
const BLACK: Vec4 = Vec4::ZERO;

pub struct ComputeEffect {
    name: String,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    pub data: ComputePushConstants,
}

pub fn create_compute_effects(
    device: &ash::Device,
    draw_image: &DrawImage,
    shader_compiler: &ShaderCompiler,
) -> eyre::Result<Vec<ComputeEffect>> {
    let gradient_effect = {
        let src = include_str!("../shaders/gradient_color.comp");
        let module = shader_compiler.create_shader_module_from_str(
            device,
            src,
            shaderc::ShaderKind::Compute,
            "gradient_color.comp",
            "main",
        )?;
        ComputeEffect::new(
            device,
            draw_image,
            "Gradient Color",
            module,
            ComputePushConstants::new(RED, BLUE, BLACK, BLACK),
        )?
    };

    let sky = {
        let src = include_str!("../shaders/sky.comp");
        let module = shader_compiler.create_shader_module_from_str(
            device,
            src,
            shaderc::ShaderKind::Compute,
            "gradient_color.comp",
            "main",
        )?;
        ComputeEffect::new(
            device,
            draw_image,
            "Sky",
            module,
            ComputePushConstants::new(Vec4::new(0.1, 0.2, 0.4, 0.97), BLACK, BLACK, BLACK),
        )?
    };
    Ok(vec![gradient_effect, sky])
}

impl ComputeEffect {
    pub fn new(
        device: &ash::Device,
        draw_image: &DrawImage,
        name: impl Into<String>,
        module: vk::ShaderModule,
        data: ComputePushConstants,
    ) -> eyre::Result<Self> {
        let push_constant = vk::PushConstantRange::default()
            .offset(0)
            .size(std::mem::size_of::<ComputePushConstants>() as u32)
            .stage_flags(vk::ShaderStageFlags::COMPUTE);
        let layouts = [draw_image.descriptor_set_layout()];
        let push_constants = [push_constant];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(&push_constants);
        let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;

        let stage = vk::PipelineShaderStageCreateInfo::default()
            .module(module)
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main");

        let info = vk::ComputePipelineCreateInfo::default()
            .layout(layout)
            .stage(stage);

        let pipeline = match unsafe {
            device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None)
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => return Err(eyre!("{e}")),
        };

        unsafe { device.destroy_shader_module(module, None) };
        Ok(Self {
            name: name.into(),
            pipeline,
            layout,
            data,
        })
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.destroy_pipeline_layout(self.layout, None) };
        unsafe { device.destroy_pipeline(self.pipeline, None) };
    }

    pub const fn pipeline(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub const fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputePushConstants {
    pub data1: Vec4,
    pub data2: Vec4,
    pub data3: Vec4,
    pub data4: Vec4,
}

impl ComputePushConstants {
    pub const fn new(data1: Vec4, data2: Vec4, data3: Vec4, data4: Vec4) -> Self {
        Self {
            data1,
            data2,
            data3,
            data4,
        }
    }
}
