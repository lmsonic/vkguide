use ash::vk;

use crate::utils::create_cmd_buffer_info;

pub struct ImmediateSubmit {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
}

impl ImmediateSubmit {
    pub fn new(device: &ash::Device, queue_index: u32) -> eyre::Result<Self> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_index);
        let pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let cmd_info = create_cmd_buffer_info().pool(pool).call();
        let cmd = unsafe { device.allocate_command_buffers(&cmd_info) }?[0];
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let fence = unsafe { device.create_fence(&fence_info, None) }?;
        Ok(Self { pool, cmd, fence })
    }
    pub fn submit(
        &self,
        device: &ash::Device,
        queue: vk::Queue,
        mut func: impl FnMut(vk::CommandBuffer),
    ) -> eyre::Result<()> {
        unsafe { device.reset_fences(&[self.fence]) }?;
        unsafe { device.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty()) }?;
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(self.cmd, &begin_info) }?;
        func(self.cmd);
        unsafe { device.end_command_buffer(self.cmd) }?;
        let cmd_info = vk::CommandBufferSubmitInfo::default()
            .command_buffer(self.cmd)
            .device_mask(0);
        let cmd_infos = [cmd_info];

        let submit_info = vk::SubmitInfo2::default().command_buffer_infos(&cmd_infos);
        unsafe { device.queue_submit2(queue, &[submit_info], self.fence) }?;
        unsafe { device.wait_for_fences(&[self.fence], true, u64::MAX) }?;
        Ok(())
    }
    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe { device.free_command_buffers(self.pool, &[self.cmd]) };
        unsafe { device.destroy_command_pool(self.pool, None) };
        unsafe { device.destroy_fence(self.fence, None) };
    }

    pub const fn pool(&self) -> vk::CommandPool {
        self.pool
    }

    pub const fn cmd(&self) -> vk::CommandBuffer {
        self.cmd
    }

    pub const fn fence(&self) -> vk::Fence {
        self.fence
    }
}
