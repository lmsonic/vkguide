use ash::vk;
use eyre::Context;
use winit::window::Window;

use crate::vulkan::Vulkan;

pub const IMAGE_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;
pub const COLOR_SPACE: vk::ColorSpaceKHR = vk::ColorSpaceKHR::SRGB_NONLINEAR;

pub struct Swapchain {
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    render_semaphores: Vec<vk::Semaphore>,
    extent: vk::Extent2D,
    format: vk::Format,
}

impl Swapchain {
    pub fn new(
        window: &Window,
        vulkan: &Vulkan,
        format: vk::Format,
        color_space: vk::ColorSpaceKHR,
        present_mode: vk::PresentModeKHR,
        add_image_usage: vk::ImageUsageFlags,
    ) -> eyre::Result<Self> {
        let surface_instance = vulkan.surface_instance();
        let physical_device = vulkan.physical_device();
        let surface = vulkan.surface();
        let surface_caps = unsafe {
            surface_instance.get_physical_device_surface_capabilities(physical_device, surface)
        }
        .wrap_err("could not get physical device surface caps")?;
        let (image_format, color_space) = unsafe {
            surface_instance.get_physical_device_surface_formats(physical_device, surface)
        }?
        .iter()
        .find_map(|f| {
            (f.format == format && f.color_space == color_space)
                .then_some((f.format, f.color_space))
        })
        .unwrap_or((IMAGE_FORMAT, COLOR_SPACE));

        let mut image_count = surface_caps.min_image_count + 1;
        if surface_caps.max_image_count > 0 && image_count > surface_caps.max_image_count {
            image_count = surface_caps.max_image_count;
        }

        let width = window.inner_size().width;
        let height = window.inner_size().height;
        let extent = if surface_caps.current_extent.width == u32::MAX {
            vk::Extent2D { width, height }
        } else {
            surface_caps.current_extent
        };
        let pre_transform = if surface_caps
            .current_transform
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_caps.current_transform
        };
        let present_modes = unsafe {
            surface_instance.get_physical_device_surface_present_modes(physical_device, surface)?
        };
        let present_mode = present_modes
            .iter()
            .copied()
            .find(|m| *m == present_mode)
            .unwrap_or(vk::PresentModeKHR::FIFO);
        let swapchain_device = vulkan.swapchain_device();

        let swapchain_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_color_space(color_space)
            .image_format(image_format)
            .image_extent(extent)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | add_image_usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);
        let swapchain = unsafe { swapchain_device.create_swapchain(&swapchain_info, None) }
            .wrap_err("could not create swapchain")?;
        let images = unsafe { swapchain_device.get_swapchain_images(swapchain) }
            .wrap_err("could not get swapchain images")?;
        let device = vulkan.device();
        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let image_views = images
            .iter()
            .filter_map(|i| {
                let info = vk::ImageViewCreateInfo::default()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(image_format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .format(image_format)
                    .image(*i)
                    .subresource_range(subresource_range);
                unsafe { device.create_image_view(&info, None).ok() }
            })
            .collect();
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let mut render_semaphores = Vec::with_capacity(images.len());
        for _ in 0..images.len() {
            render_semaphores.push(unsafe { device.create_semaphore(&semaphore_info, None) }?);
        }
        Ok(Self {
            swapchain,
            images,
            image_views,
            render_semaphores,
            extent,
            format: image_format,
        })
    }
    pub fn destroy(
        &mut self,
        device: &ash::Device,
        swapchain_device: &ash::khr::swapchain::Device,
    ) {
        unsafe { swapchain_device.destroy_swapchain(self.swapchain, None) };
        for v in &self.image_views {
            unsafe { device.destroy_image_view(*v, None) };
        }
        for s in &self.render_semaphores {
            unsafe { device.destroy_semaphore(*s, None) };
        }
    }

    pub const fn swapchain(&self) -> vk::SwapchainKHR {
        self.swapchain
    }

    pub fn images(&self) -> &[vk::Image] {
        &self.images
    }

    pub fn image_views(&self) -> &[vk::ImageView] {
        &self.image_views
    }

    pub fn render_semaphores(&self) -> &[vk::Semaphore] {
        &self.render_semaphores
    }

    pub const fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    pub const fn format(&self) -> vk::Format {
        self.format
    }
}
