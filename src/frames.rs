use ash::vk;

use crate::{utils, vulkan::Vulkan};

pub const FRAMES_IN_FLIGHT: usize = 2;

pub struct Frames {
    frames: [FrameData; FRAMES_IN_FLIGHT],
    frame_index: usize,
}

impl Frames {
    pub fn new(vulkan: &Vulkan) -> eyre::Result<Self> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(vulkan.queue_family_indices().graphics);

        let mut frames = [const { FrameData::uninit() }; FRAMES_IN_FLIGHT];
        let device = vulkan.device();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        for frame_data in &mut frames {
            let pool = unsafe { device.create_command_pool(&pool_info, None) }?;
            let buffer_info = utils::create_cmd_buffer_info().pool(pool).call();

            let buffer = unsafe { device.allocate_command_buffers(&buffer_info) }?;
            frame_data.cmd_pool = pool;
            frame_data.cmd_buffer = buffer[0];
            frame_data.render_fence = unsafe { device.create_fence(&fence_info, None) }?;
            frame_data.swapchain_semaphore =
                unsafe { device.create_semaphore(&semaphore_info, None) }?;
        }
        Ok(Self {
            frames,
            frame_index: 0,
        })
    }

    pub const fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.frame_index % FRAMES_IN_FLIGHT]
    }
    pub const fn advance(&mut self) {
        self.frame_index += 1;
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        for f in &mut self.frames {
            f.destroy(device);
        }
    }
}

pub struct FrameData {
    cmd_pool: vk::CommandPool,
    cmd_buffer: vk::CommandBuffer,
    render_fence: vk::Fence,
    swapchain_semaphore: vk::Semaphore,
}

impl FrameData {
    const fn uninit() -> Self {
        Self {
            cmd_pool: vk::CommandPool::null(),
            cmd_buffer: vk::CommandBuffer::null(),
            render_fence: vk::Fence::null(),
            swapchain_semaphore: vk::Semaphore::null(),
        }
    }

    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.destroy_command_pool(self.cmd_pool, None) };
        unsafe { device.destroy_fence(self.render_fence, None) };
        unsafe { device.destroy_semaphore(self.swapchain_semaphore, None) };
    }

    pub const fn cmd_pool(&self) -> vk::CommandPool {
        self.cmd_pool
    }

    pub const fn cmd_buffer(&self) -> vk::CommandBuffer {
        self.cmd_buffer
    }

    pub const fn render_fence(&self) -> vk::Fence {
        self.render_fence
    }

    pub const fn swapchain_semaphore(&self) -> vk::Semaphore {
        self.swapchain_semaphore
    }
}
