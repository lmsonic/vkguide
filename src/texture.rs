use ash::vk::{self};
use glam::Vec4;
use vk_mem::Alloc;

use crate::{
    buffer::AllocatedBuffer,
    descriptors::{DescriptorAllocator, DescriptorLayoutBuilder, DescriptorWriter},
    immediate::ImmediateSubmit,
    utils::{
        image_subresource_range, layout_to_flag, memcopy, pack_unorm_4x8, transition_image,
        transition_image_queue,
    },
    vulkan::QueueFamilyIndices,
};

pub const WHITE: Vec4 = Vec4::ONE;
pub const GREY: Vec4 = Vec4::new(0.66, 0.66, 0.66, 1.0);
pub const BLACK: Vec4 = Vec4::ZERO;
pub const MAGENTA: Vec4 = Vec4::new(1.0, 0.0, 1.0, 1.0);

pub struct EngineImages {
    pub white: AllocatedImage,
    pub grey: AllocatedImage,
    pub black: AllocatedImage,
    pub error: AllocatedImage,
}

impl EngineImages {
    pub fn new(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        immediate_graphics: &ImmediateSubmit,
        graphics_queue: vk::Queue,
    ) -> eyre::Result<Self> {
        let color_format = vk::Format::R8G8B8A8_UNORM;

        let allocate_image = |pixels, extent| {
            AllocatedImage::with_data(
                pixels,
                device,
                allocator,
                immediate_graphics,
                graphics_queue,
                color_format,
                extent,
                vk::ImageUsageFlags::SAMPLED,
                false,
            )
        };

        let extent_1px = vk::Extent3D {
            width: 1,
            height: 1,
            depth: 1,
        };
        let white_color = [pack_unorm_4x8(WHITE)];
        let white = allocate_image(&white_color, extent_1px)?;
        let grey_color = [pack_unorm_4x8(GREY)];
        let grey = allocate_image(&grey_color, extent_1px)?;

        let black_color = pack_unorm_4x8(BLACK);
        let binding = [black_color];
        let black = allocate_image(&binding, extent_1px)?;

        let magenta = pack_unorm_4x8(MAGENTA);
        const CHECKER_SIZE: usize = 16;
        let mut pixels = [0_u32; CHECKER_SIZE * CHECKER_SIZE];
        for x in 0..CHECKER_SIZE {
            for y in 0..CHECKER_SIZE {
                pixels[y * CHECKER_SIZE + x] = if ((x % 2) ^ (y % 2)) != 0 {
                    magenta
                } else {
                    black_color
                };
            }
        }
        let error = allocate_image(
            &pixels,
            vk::Extent3D {
                width: CHECKER_SIZE as u32,
                height: CHECKER_SIZE as u32,
                depth: 1,
            },
        )?;
        Ok(Self {
            white,
            grey,
            black,
            error,
        })
    }
    pub fn destroy(&mut self, device: &ash::Device, allocator: &vk_mem::Allocator) {
        self.white.destroy(device, allocator);
        self.grey.destroy(device, allocator);
        self.black.destroy(device, allocator);
        self.error.destroy(device, allocator);
    }
}

pub struct DefaultSamplers {
    pub nearest: vk::Sampler,
    pub linear: vk::Sampler,
}

impl DefaultSamplers {
    pub fn new(device: &ash::Device) -> eyre::Result<Self> {
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST);
        let nearest = unsafe { device.create_sampler(&sampler_info, None) }?;
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR);
        let linear = unsafe { device.create_sampler(&sampler_info, None) }?;
        Ok(Self { nearest, linear })
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.destroy_sampler(self.nearest, None) };
        unsafe { device.destroy_sampler(self.linear, None) };
    }
}

pub fn copy_image_to_image(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    dst: vk::Image,
    src_size: vk::Extent2D,
    dst_size: vk::Extent2D,
) {
    let subresource = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_array_layer(0)
        .layer_count(1)
        .mip_level(0);
    let region = vk::ImageBlit2::default()
        .src_offsets([
            vk::Offset3D::default(),
            vk::Offset3D {
                x: src_size.width.cast_signed(),
                y: src_size.height.cast_signed(),
                z: 1,
            },
        ])
        .dst_offsets([
            vk::Offset3D::default(),
            vk::Offset3D {
                x: dst_size.width.cast_signed(),
                y: dst_size.height.cast_signed(),
                z: 1,
            },
        ])
        .src_subresource(subresource)
        .dst_subresource(subresource);
    let regions = [region];
    let blit_info = vk::BlitImageInfo2::default()
        .src_image(src)
        .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .dst_image(dst)
        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .filter(vk::Filter::LINEAR)
        .regions(&regions);
    unsafe { device.cmd_blit_image2(cmd, &blit_info) };
}

pub fn image_create_info<'a>(
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    extent: vk::Extent3D,
) -> vk::ImageCreateInfo<'a> {
    vk::ImageCreateInfo::default()
        .format(format)
        .image_type(vk::ImageType::TYPE_2D)
        .usage(usage)
        .extent(extent)
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
}
pub fn image_view_create_info<'a>(
    format: vk::Format,
    image: vk::Image,
    aspect_flags: vk::ImageAspectFlags,
) -> vk::ImageViewCreateInfo<'a> {
    vk::ImageViewCreateInfo::default()
        .format(format)
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(aspect_flags)
                .layer_count(1)
                .level_count(1),
        )
}

pub struct DrawImage {
    image: AllocatedImage,
    descriptor_set: vk::DescriptorSet,
    descriptor_set_layout: vk::DescriptorSetLayout,
}

impl std::ops::Deref for DrawImage {
    type Target = AllocatedImage;

    fn deref(&self) -> &Self::Target {
        &self.image
    }
}

impl DrawImage {
    pub fn new(
        width: u32,
        height: u32,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        descriptor_allocator: &DescriptorAllocator,
    ) -> eyre::Result<Self> {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };
        let image = AllocatedImage::create_draw_image(device, allocator, extent)?;
        let descriptor_set_layout = DescriptorLayoutBuilder::new()
            .add_binding(0, vk::DescriptorType::STORAGE_IMAGE)
            .build(device, vk::ShaderStageFlags::COMPUTE)?;
        let set = descriptor_allocator.allocate(device, descriptor_set_layout)?[0];

        DescriptorWriter::new()
            .write_image(
                0,
                image.image_view,
                vk::Sampler::null(),
                vk::ImageLayout::GENERAL,
                vk::DescriptorType::STORAGE_IMAGE,
            )
            .update_set(device, set);

        Ok(Self {
            image,
            descriptor_set: set,
            descriptor_set_layout,
        })
    }
    pub fn destroy(&mut self, device: &ash::Device, allocator: &vk_mem::Allocator) {
        unsafe { device.destroy_descriptor_set_layout(self.descriptor_set_layout, None) };
        self.image.destroy(device, allocator);
    }

    pub const fn allocated_image(&self) -> &AllocatedImage {
        &self.image
    }

    pub const fn descriptor_set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    pub const fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }
}
pub struct AllocatedImage {
    image: vk::Image,
    image_view: vk::ImageView,
    allocation: vk_mem::Allocation,
    extent: vk::Extent3D,
    format: vk::Format,
}
impl AllocatedImage {
    pub fn create_depth_image(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        draw_image: &DrawImage,
    ) -> Result<Self, eyre::Error> {
        let format = vk::Format::D32_SFLOAT;
        let extent = draw_image.extent();
        let usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;

        Self::new(device, allocator, format, extent, usage, false)
    }
    fn create_draw_image(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        extent: vk::Extent3D,
    ) -> eyre::Result<Self> {
        let format = vk::Format::R16G16B16A16_SFLOAT;

        let usage = vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::COLOR_ATTACHMENT;

        Self::new(device, allocator, format, extent, usage, false)
    }

    pub fn new(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: vk::Extent3D,
        usage: vk::ImageUsageFlags,
        mipmapped: bool,
    ) -> eyre::Result<Self> {
        let mut image_info = image_create_info(format, usage, extent);
        if mipmapped {
            let mip_levels = (extent.width.max(extent.height) as f32).log2().floor() as u32 + 1;
            image_info = image_info.mip_levels(mip_levels);
        }
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        let (image, allocation) = unsafe { allocator.create_image(&image_info, &alloc_info) }?;

        let aspect_flags = if format == vk::Format::D32_SFLOAT {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let mut image_view_info = image_view_create_info(format, image, aspect_flags);
        image_view_info.subresource_range = image_view_info
            .subresource_range
            .level_count(image_info.mip_levels);

        let image_view = unsafe { device.create_image_view(&image_view_info, None) }?;

        Ok(Self {
            image,
            image_view,
            allocation,
            extent,
            format,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn with_data(
        data: &[u32],
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        immediate_graphics: &ImmediateSubmit,
        graphics_queue: vk::Queue,
        format: vk::Format,
        extent: vk::Extent3D,
        usage: vk::ImageUsageFlags,
        mipmapped: bool,
    ) -> eyre::Result<Self> {
        let size = extent.depth * extent.width * extent.height * std::mem::size_of::<u32>() as u32;
        debug_assert!(data.len() == (extent.depth * extent.width * extent.height) as usize);
        let mut staging_buffer = AllocatedBuffer::new(
            allocator,
            u64::from(size),
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk_mem::MemoryUsage::Auto,
        )?;
        let memory = unsafe { allocator.map_memory(&mut staging_buffer.allocation()) }?;

        unsafe { memcopy(data, memory) };

        let image = Self::new(
            device,
            allocator,
            format,
            extent,
            usage | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST,
            mipmapped,
        )?;

        immediate_graphics.submit(device, graphics_queue, |cmd| {
            transition_image(
                device,
                cmd,
                image.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            let image_subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            let copy = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_extent(extent)
                .image_subresource(image_subresource);
            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging_buffer.buffer(),
                    image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
            };
            transition_image(
                device,
                cmd,
                image.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        })?;
        unsafe { allocator.unmap_memory(&mut staging_buffer.allocation()) };
        staging_buffer.destroy(allocator);
        Ok(image)
    }

    pub fn destroy(&mut self, device: &ash::Device, allocator: &vk_mem::Allocator) {
        unsafe { device.destroy_image_view(self.image_view, None) };
        unsafe { allocator.destroy_image(self.image, &mut self.allocation) };
    }

    pub const fn image(&self) -> vk::Image {
        self.image
    }

    pub const fn image_view(&self) -> vk::ImageView {
        self.image_view
    }

    pub const fn allocation(&self) -> vk_mem::Allocation {
        self.allocation
    }

    pub const fn extent(&self) -> vk::Extent3D {
        self.extent
    }

    pub const fn format(&self) -> vk::Format {
        self.format
    }
}
