use ash::vk;

pub fn semaphore_submit_info<'a>(
    stage_mask: vk::PipelineStageFlags2,
    semaphore: vk::Semaphore,
) -> vk::SemaphoreSubmitInfo<'a> {
    vk::SemaphoreSubmitInfo::default()
        .semaphore(semaphore)
        .stage_mask(stage_mask)
        .device_index(0)
        .value(1)
}

pub fn transition_image(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) {
    let subresource_range =
        image_subresource_range(if new_layout == vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        });
    let image_barrier = vk::ImageMemoryBarrier2::default()
        .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
        .dst_access_mask(vk::AccessFlags2::MEMORY_WRITE | vk::AccessFlags2::MEMORY_READ)
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .subresource_range(subresource_range)
        .image(image);
    let image_barriers = [image_barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&image_barriers);
    unsafe { device.cmd_pipeline_barrier2(cmd, &dependency) };
}

pub fn image_subresource_range(aspect_flags: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspect_flags)
        .level_count(vk::REMAINING_MIP_LEVELS)
        .layer_count(vk::REMAINING_ARRAY_LAYERS)
}

pub fn create_cmd_buffer_info<'a>(
    pool: vk::CommandPool,
    count: u32,
) -> vk::CommandBufferAllocateInfo<'a> {
    vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count)
}
