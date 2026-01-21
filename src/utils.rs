use ash::vk;

/// # Safety
///
/// Memory needs to be allocated for bytes size
//
pub const unsafe fn memcopy<T>(buffer: &[T], memory: *mut u8) {
    unsafe {
        std::ptr::copy_nonoverlapping(buffer.as_ptr(), memory.cast(), buffer.len());
    };
}

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
    fn layout_to_flag(layout: vk::ImageLayout) -> vk::AccessFlags2 {
        match layout {
            vk::ImageLayout::TRANSFER_DST_OPTIMAL => vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL => vk::AccessFlags2::TRANSFER_READ,
            vk::ImageLayout::PRESENT_SRC_KHR => vk::AccessFlags2::empty(),
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => {
                vk::AccessFlags2::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags2::COLOR_ATTACHMENT_READ_NONCOHERENT_EXT
            }
            _ => vk::AccessFlags2::MEMORY_WRITE | vk::AccessFlags2::MEMORY_READ,
        }
    }
    let subresource_range =
        image_subresource_range(if new_layout == vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        });
    let image_barrier = vk::ImageMemoryBarrier2::default()
        .src_access_mask(layout_to_flag(old_layout))
        .dst_access_mask(layout_to_flag(new_layout))
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

#[bon::builder]
pub fn create_cmd_buffer_info<'a>(
    pool: vk::CommandPool,
    count: Option<u32>,
) -> vk::CommandBufferAllocateInfo<'a> {
    vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count.unwrap_or(1))
}

#[bon::builder]
pub fn color_attachment_info<'a>(
    view: vk::ImageView,
    clear: Option<vk::ClearValue>,
    layout: Option<vk::ImageLayout>,
) -> ash::vk::RenderingAttachmentInfo<'a> {
    let mut info = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(layout.unwrap_or(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL))
        .load_op(if clear.is_some() {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::LOAD
        })
        .store_op(vk::AttachmentStoreOp::STORE);
    if let Some(clear) = clear {
        info.clear_value = clear;
    }
    info
}

#[bon::builder]
pub fn rendering_info<'a>(
    render_extent: vk::Extent2D,
    color_attachments: &'a [vk::RenderingAttachmentInfo<'a>],
    depth_attachment: Option<vk::RenderingAttachmentInfo<'a>>,
) -> vk::RenderingInfo<'a> {
    let mut info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: render_extent,
        })
        .layer_count(1)
        .color_attachments(color_attachments);

    if let Some(depth) = depth_attachment {
        info.p_depth_attachment = &raw const depth;
    }

    info
}
