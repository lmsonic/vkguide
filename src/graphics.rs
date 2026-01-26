use ash::vk::{self};
use eyre::eyre;

use crate::{
    descriptors::DescriptorLayoutBuilder,
    mesh::GPUDrawPushConstants,
    shader::ShaderCompiler,
    texture::{AllocatedImage, DrawImage},
};

pub struct MeshPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl MeshPipeline {
    pub fn new(
        device: &ash::Device,
        shader_compiler: &ShaderCompiler,
        draw_image: &DrawImage,
        depth_image: &AllocatedImage,
        image_layout: vk::DescriptorSetLayout,
    ) -> eyre::Result<Self> {
        let vertex_src = include_str!("../shaders/colored_triangle_mesh.vert");
        let vertex_shader = shader_compiler.create_shader_module_from_str(
            device,
            vertex_src,
            shaderc::ShaderKind::Vertex,
            "colored_triangle_mesh.vert",
            "main",
        )?;

        let frag_src = include_str!("../shaders/tex_image.frag");
        let frag_shader = shader_compiler.create_shader_module_from_str(
            device,
            frag_src,
            shaderc::ShaderKind::Fragment,
            "tex_image.frag",
            "main",
        )?;
        let push_constant = vk::PushConstantRange::default()
            .size(std::mem::size_of::<GPUDrawPushConstants>() as u32)
            .stage_flags(vk::ShaderStageFlags::VERTEX);

        let push_constants = [push_constant];
        let set_layouts = [image_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(&push_constants)
            .set_layouts(&set_layouts);
        let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }?;

        let pipeline = GraphicsPipelineInfo::builder()
            .layout(layout)
            .shaders([vertex_shader, frag_shader])
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .color_attachment_format(draw_image.format())
            .depth_format(depth_image.format())
            .depth_enabled(true)
            .blending(Blending::Alpha)
            .build()
            .create(device)?;

        unsafe { device.destroy_shader_module(vertex_shader, None) };
        unsafe { device.destroy_shader_module(frag_shader, None) };
        Ok(Self { pipeline, layout })
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
}

#[derive(Clone, Copy)]
pub enum Blending {
    Additive,
    Alpha,
}

#[derive(bon::Builder)]
pub struct GraphicsPipelineInfo {
    shaders: [vk::ShaderModule; 2],
    topology: vk::PrimitiveTopology,
    polygon_mode: vk::PolygonMode,
    cull_mode: vk::CullModeFlags,
    front_face: vk::FrontFace,
    color_attachment_format: vk::Format,
    depth_format: vk::Format,
    layout: vk::PipelineLayout,
    depth_enabled: bool,
    depth_write_enabled: Option<bool>,
    depth_compare_op: Option<vk::CompareOp>,
    blending: Option<Blending>,
}

impl GraphicsPipelineInfo {
    fn shader_stages(&self) -> [vk::PipelineShaderStageCreateInfo<'_>; 2] {
        let vertex = vk::PipelineShaderStageCreateInfo::default()
            .module(self.shaders[0])
            .name(c"main")
            .stage(vk::ShaderStageFlags::VERTEX);
        let fragment = vk::PipelineShaderStageCreateInfo::default()
            .module(self.shaders[1])
            .name(c"main")
            .stage(vk::ShaderStageFlags::FRAGMENT);
        [vertex, fragment]
    }
    fn input_assembly(&self) -> vk::PipelineInputAssemblyStateCreateInfo<'_> {
        vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(self.topology)
            .primitive_restart_enable(false)
    }
    fn rasterizer(&self) -> vk::PipelineRasterizationStateCreateInfo<'_> {
        vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(self.polygon_mode)
            .line_width(1.0)
            .cull_mode(self.cull_mode)
            .front_face(self.front_face)
    }

    fn create(self, device: &ash::Device) -> eyre::Result<vk::Pipeline> {
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);

        let color_attachment =
            self.blending
                .map_or_else(disable_blending, |blending| match blending {
                    Blending::Additive => additive_blending(),
                    Blending::Alpha => alpha_blending(),
                });
        let attachments = [color_attachment];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&attachments);
        let vertex_info = vk::PipelineVertexInputStateCreateInfo::default();

        let color_attachment_formats = [self.color_attachment_format];
        let mut render_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_attachment_formats)
            .depth_attachment_format(self.depth_format);

        let shader_stages = self.shader_stages();
        let input_assembly = self.input_assembly();
        let rasterizer = self.rasterizer();
        let multisampling = disable_multisampling();
        let depth_stencil_state = if self.depth_enabled {
            enable_depth_test(
                self.depth_write_enabled.unwrap_or(true),
                self.depth_compare_op
                    .unwrap_or(vk::CompareOp::GREATER_OR_EQUAL),
            )
        } else {
            disable_depth_test()
        };

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .push_next(&mut render_info)
            .stages(&shader_stages)
            .vertex_input_state(&vertex_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blend)
            .depth_stencil_state(&depth_stencil_state)
            .layout(self.layout)
            .dynamic_state(&dynamic_state);
        let pipeline = match unsafe {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => return Err(eyre!("{e}")),
        };
        Ok(pipeline)
    }
}

fn disable_multisampling<'a>() -> vk::PipelineMultisampleStateCreateInfo<'a> {
    vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .min_sample_shading(1.0)
        .sample_mask(&[])
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false)
}

fn disable_blending() -> vk::PipelineColorBlendAttachmentState {
    vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false)
}

fn additive_blending() -> vk::PipelineColorBlendAttachmentState {
    // src.rgb * src.a + dst.rgc * 1.0
    vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)
}
fn alpha_blending() -> vk::PipelineColorBlendAttachmentState {
    // src.rgb * src.a + dst.rgb * (1.0 - dst.a)
    vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_DST_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)
}

fn disable_depth_test<'a>() -> vk::PipelineDepthStencilStateCreateInfo<'a> {
    vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::NEVER)
        .depth_bounds_test_enable(false)
        .min_depth_bounds(0.0)
        .max_depth_bounds(1.0)
}

fn enable_depth_test<'a>(
    depth_write_enabled: bool,
    compare_op: vk::CompareOp,
) -> vk::PipelineDepthStencilStateCreateInfo<'a> {
    vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(depth_write_enabled)
        .depth_compare_op(compare_op)
        .depth_bounds_test_enable(false)
        .min_depth_bounds(0.0)
        .max_depth_bounds(1.0)
}
