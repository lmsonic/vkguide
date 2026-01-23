use ash::vk;
use vk_mem::Alloc;
use winit::window::Window;

use crate::descriptors::{DescriptorAllocator, DescriptorLayoutBuilder};

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

pub struct DepthImage {
    image: AllocatedImage,
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

pub fn create_depth_image(
    device: &ash::Device,
    allocator: &vk_mem::Allocator,
    draw_image: &DrawImage,
) -> Result<AllocatedImage, eyre::Error> {
    AllocatedImage::new(
        device,
        allocator,
        vk::Format::D32_SFLOAT,
        draw_image.extent(),
        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        vk::ImageAspectFlags::DEPTH,
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        },
    )
}

impl DrawImage {
    pub fn new(
        window: &Window,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        descriptor_allocator: &DescriptorAllocator,
    ) -> eyre::Result<Self> {
        let format = vk::Format::R16G16B16A16_SFLOAT;
        let extent = vk::Extent3D {
            width: window.inner_size().width,
            height: window.inner_size().height,
            depth: 1,
        };
        let usage = vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::COLOR_ATTACHMENT;

        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        let image = AllocatedImage::new(
            device,
            allocator,
            format,
            extent,
            usage,
            vk::ImageAspectFlags::COLOR,
            &alloc_info,
        )?;
        let descriptor_set_layout = DescriptorLayoutBuilder::new()
            .add_binding(0, vk::DescriptorType::STORAGE_IMAGE)
            .build(device, vk::ShaderStageFlags::COMPUTE)?;
        let descriptor_set = descriptor_allocator.allocate(device, descriptor_set_layout)?[0];
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::GENERAL)
            .image_view(image.image_view());
        let image_infos = [image_info];
        let write_descriptor_set = vk::WriteDescriptorSet::default()
            .dst_binding(0)
            .dst_set(descriptor_set)
            .image_info(&image_infos)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE);
        unsafe { device.update_descriptor_sets(&[write_descriptor_set], &[]) };
        Ok(Self {
            image,
            descriptor_set,
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
    pub fn new(
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: vk::Extent3D,
        usage: vk::ImageUsageFlags,
        aspect_flags: vk::ImageAspectFlags,
        alloc_info: &vk_mem::AllocationCreateInfo,
    ) -> eyre::Result<Self> {
        let image_info = image_create_info(format, usage, extent);

        let (image, allocation) = unsafe { allocator.create_image(&image_info, &alloc_info) }?;

        let image_view_info = image_view_create_info(format, image, aspect_flags);
        let image_view = unsafe { device.create_image_view(&image_view_info, None) }?;

        Ok(Self {
            image,
            image_view,
            allocation,
            extent,
            format,
        })
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
