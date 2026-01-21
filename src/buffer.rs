use ash::vk;
use vk_mem::Alloc;

pub struct AllocatedBuffer {
    buffer: vk::Buffer,
    allocation: vk_mem::Allocation,
    alloc_info: vk_mem::AllocationInfo,
}

impl AllocatedBuffer {
    pub fn new(
        allocator: &vk_mem::Allocator,
        size: u64,
        usage: vk::BufferUsageFlags,
        mem_usage: vk_mem::MemoryUsage,
    ) -> eyre::Result<Self> {
        let additional_flags = match mem_usage {
            vk_mem::MemoryUsage::Auto
            | vk_mem::MemoryUsage::AutoPreferHost
            | vk_mem::MemoryUsage::AutoPreferDevice => {
                vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
            }

            _ => vk_mem::AllocationCreateFlags::empty(),
        };
        let info = vk::BufferCreateInfo::default().usage(usage).size(size);
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: mem_usage,
            flags: vk_mem::AllocationCreateFlags::MAPPED | additional_flags,
            ..Default::default()
        };
        let (buffer, allocation) = unsafe { allocator.create_buffer(&info, &alloc_info) }?;
        let alloc_info = allocator.get_allocation_info(&allocation);
        Ok(Self {
            buffer,
            allocation,
            alloc_info,
        })
    }
    pub fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        unsafe { allocator.destroy_buffer(self.buffer, &mut self.allocation) };
    }

    pub const fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    pub const fn allocation(&self) -> vk_mem::Allocation {
        self.allocation
    }

    pub const fn alloc_info(&self) -> &vk_mem::AllocationInfo {
        &self.alloc_info
    }
}
