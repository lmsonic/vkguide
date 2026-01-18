use ash::vk;
use eyre::eyre;

use crate::{shader::ShaderCompiler, texture::DrawImage};

pub struct BackgroundPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl BackgroundPipeline {
    pub fn new(
        device: &ash::Device,
        shader_compiler: &ShaderCompiler,
        draw_image: &DrawImage,
    ) -> eyre::Result<Self> {
        let layouts = [draw_image.descriptor_set_layout()];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts);
        let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }?;
        let shader_str = include_str!("../shaders/gradient.comp");
        let module = shader_compiler.create_shader_module_from_str(
            device,
            shader_str,
            shaderc::ShaderKind::Compute,
            "gradient.comp",
            "main",
        )?;
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
        Ok(Self { pipeline, layout })
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.destroy_pipeline_layout(self.layout, None) };
        unsafe { device.destroy_pipeline(self.pipeline, None) };
    }
}
