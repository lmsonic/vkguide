use ash::vk;

use crate::{utils, vulkan::Vulkan};

pub const FRAMES_IN_FLIGHT: usize = 2;

pub fn create_frames(vulkan: &Vulkan) -> eyre::Result<[FrameData; FRAMES_IN_FLIGHT]> {
    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(vulkan.queue_family_indices().graphics);

    let mut frame_datas = [FrameData::default(); FRAMES_IN_FLIGHT];
    let device = vulkan.device();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    for frame_data in &mut frame_datas {
        let pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let buffer_info = utils::create_cmd_buffer_info(pool, 1);

        let buffer = unsafe { device.allocate_command_buffers(&buffer_info) }?;
        frame_data.cmd_pool = pool;
        frame_data.cmd_buffer = buffer[0];
        frame_data.render_fence = unsafe { device.create_fence(&fence_info, None) }?;
        frame_data.swapchain_semaphore = unsafe { device.create_semaphore(&semaphore_info, None) }?;
    }
    Ok(frame_datas)
}

#[derive(Default, Clone, Copy)]
pub struct FrameData {
    cmd_pool: vk::CommandPool,
    cmd_buffer: vk::CommandBuffer,
    render_fence: vk::Fence,
    swapchain_semaphore: vk::Semaphore,
}

impl FrameData {
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
