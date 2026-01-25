use ash::vk::{self, ExtendsDescriptorSetAllocateInfo, ExtendsDescriptorSetLayoutCreateInfo};
use eyre::eyre;
use typed_arena::Arena;

pub struct DescriptorLayoutBuilder<'a, 'b> {
    bindings: Vec<vk::DescriptorSetLayoutBinding<'a>>,
    next: Option<&'b mut dyn ExtendsDescriptorSetLayoutCreateInfo>,
}

impl<'b> DescriptorLayoutBuilder<'_, 'b> {
    pub const fn new() -> Self {
        Self {
            bindings: vec![],
            next: None,
        }
    }

    pub fn add_binding(mut self, binding: u32, descriptor_type: vk::DescriptorType) -> Self {
        self.bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(descriptor_type)
                .descriptor_count(1),
        );
        self
    }
    pub fn push_next<T: ExtendsDescriptorSetLayoutCreateInfo + Sized>(
        mut self,
        next: &'b mut T,
    ) -> Self {
        self.next = Some(next);
        self
    }
    pub fn clear(&mut self) {
        self.bindings.clear();
    }
    pub fn build(
        mut self,
        device: &ash::Device,
        shader_stage: vk::ShaderStageFlags,
    ) -> Result<vk::DescriptorSetLayout, vk::Result> {
        for b in &mut self.bindings {
            b.stage_flags |= shader_stage;
        }

        let mut info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&self.bindings);
        if let Some(next) = self.next {
            info = info.push_next(next);
        }

        unsafe { device.create_descriptor_set_layout(&info, None) }
    }
}

pub struct DescriptorWriter<'a> {
    image_infos: Arena<vk::DescriptorImageInfo>,
    buffer_infos: Arena<vk::DescriptorBufferInfo>,
    writes: Vec<vk::WriteDescriptorSet<'a>>,
}

impl DescriptorWriter<'_> {
    pub fn new() -> Self {
        Self {
            image_infos: Arena::new(),
            buffer_infos: Arena::new(),
            writes: vec![],
        }
    }

    pub fn write_image(
        &mut self,
        binding: u32,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
        layout: vk::ImageLayout,
        descriptor_type: vk::DescriptorType,
    ) {
        let info = self.image_infos.alloc(
            vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(image_view)
                .image_layout(layout),
        );

        let mut write = vk::WriteDescriptorSet::default()
            .dst_binding(binding)
            .descriptor_count(1)
            .descriptor_type(descriptor_type);
        write.p_image_info = info;
        self.writes.push(write);
    }
    pub fn write_buffer(
        &mut self,
        binding: u32,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
        descriptor_type: vk::DescriptorType,
    ) {
        let info = self.buffer_infos.alloc(
            vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(offset)
                .range(size),
        );

        let mut write = vk::WriteDescriptorSet::default()
            .dst_binding(binding)
            .descriptor_count(1)
            .descriptor_type(descriptor_type);
        write.p_buffer_info = info;
        self.writes.push(write);
    }
    pub fn clear(self) -> Self {
        drop(self);
        Self::new()
    }
    pub fn update_set(&mut self, device: &ash::Device, set: vk::DescriptorSet) {
        for w in &mut self.writes {
            *w = w.dst_set(set);
        }
        unsafe { device.update_descriptor_sets(&self.writes, &[]) };
    }
}

#[derive(Clone)]
pub struct PoolSizeRatio {
    descriptor_type: vk::DescriptorType,
    ratio: f32,
}

impl PoolSizeRatio {
    pub const fn new(descriptor_type: vk::DescriptorType, ratio: f32) -> Self {
        Self {
            descriptor_type,
            ratio,
        }
    }
}
pub struct DescriptorAllocatorGrowable {
    ratios: Vec<PoolSizeRatio>,
    full_pool: Vec<vk::DescriptorPool>,
    ready_pool: Vec<vk::DescriptorPool>,
    pool_capacity: u32,
}

impl DescriptorAllocatorGrowable {
    pub const fn uninit() -> Self {
        Self {
            ratios: vec![],
            full_pool: vec![],
            ready_pool: vec![],
            pool_capacity: 0,
        }
    }
    pub fn new(
        device: &ash::Device,
        max_sets: u32,
        ratios: &[PoolSizeRatio],
    ) -> eyre::Result<Self> {
        let mut pool_capacity = max_sets;
        pool_capacity += pool_capacity / 2;
        pool_capacity = pool_capacity.min(4092);
        let pool = Self::create_pool(device, max_sets, ratios)?;
        Ok(Self {
            ratios: ratios.to_vec(),
            full_pool: vec![],
            ready_pool: vec![pool],
            pool_capacity,
        })
    }
    pub fn clear_pools(&mut self, device: &ash::Device) -> eyre::Result<()> {
        for p in &self.ready_pool {
            unsafe { device.reset_descriptor_pool(*p, vk::DescriptorPoolResetFlags::empty()) }?;
        }
        for p in &self.full_pool {
            unsafe { device.reset_descriptor_pool(*p, vk::DescriptorPoolResetFlags::empty()) }?;
            self.ready_pool.push(*p);
        }
        self.full_pool.clear();

        Ok(())
    }
    pub fn destroy_pools(&mut self, device: &ash::Device) {
        for p in &self.ready_pool {
            unsafe { device.destroy_descriptor_pool(*p, None) };
        }
        self.ready_pool.clear();
        for p in &self.full_pool {
            unsafe { device.destroy_descriptor_pool(*p, None) };
        }
        self.full_pool.clear();
    }
    pub fn allocate(
        &mut self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
    ) -> eyre::Result<vk::DescriptorSet> {
        let mut pool = self.get_pool(device)?;
        let layouts = [layout];
        let mut alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        let sets = match unsafe { device.allocate_descriptor_sets(&alloc_info) } {
            Ok(set) => set,
            Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY | vk::Result::ERROR_FRAGMENTED_POOL) => {
                self.full_pool.push(pool);
                pool = self.get_pool(device)?;
                alloc_info = alloc_info.descriptor_pool(pool);

                unsafe { device.allocate_descriptor_sets(&alloc_info) }?
            }
            Err(e) => return Err(eyre!("{e}")),
        };
        self.ready_pool.push(pool);
        Ok(sets[0])
    }

    pub fn allocate_push_next<T>(
        &mut self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        next: &mut T,
    ) -> eyre::Result<vk::DescriptorSet>
    where
        T: ExtendsDescriptorSetAllocateInfo + Sized,
    {
        let mut pool = self.get_pool(device)?;
        let layouts = [layout];
        let mut alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts)
            .push_next(next);

        let sets = match unsafe { device.allocate_descriptor_sets(&alloc_info) } {
            Ok(set) => set,
            Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY | vk::Result::ERROR_FRAGMENTED_POOL) => {
                self.full_pool.push(pool);
                pool = self.get_pool(device)?;
                alloc_info = alloc_info.descriptor_pool(pool);

                unsafe { device.allocate_descriptor_sets(&alloc_info) }?
            }
            Err(e) => return Err(eyre!("{e}")),
        };
        self.ready_pool.push(pool);
        Ok(sets[0])
    }

    fn get_pool(&mut self, device: &ash::Device) -> eyre::Result<vk::DescriptorPool> {
        if let Some(pool) = self.ready_pool.pop() {
            Ok(pool)
        } else {
            let pool = Self::create_pool(device, self.pool_capacity, &self.ratios)?;
            self.pool_capacity += self.pool_capacity / 2;
            self.pool_capacity = self.pool_capacity.min(4092);
            Ok(pool)
        }
    }
    fn create_pool(
        device: &ash::Device,
        set_count: u32,
        ratios: &[PoolSizeRatio],
    ) -> eyre::Result<vk::DescriptorPool> {
        // let mut sum = 0;

        let pool_sizes = ratios
            .iter()
            .map(|r| {
                // sum += size.descriptor_count;
                vk::DescriptorPoolSize::default()
                    .ty(r.descriptor_type)
                    .descriptor_count((r.ratio * set_count as f32) as u32)
            })
            .collect::<Vec<_>>();
        // debug_assert_eq!(sum, set_count);
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(set_count)
            .pool_sizes(&pool_sizes);
        let pool = unsafe { device.create_descriptor_pool(&info, None) }?;
        Ok(pool)
    }
}

pub struct DescriptorAllocator {
    pool: vk::DescriptorPool,
}

impl DescriptorAllocator {
    pub fn new(
        device: &ash::Device,
        max_sets: u32,
        ratios: &[PoolSizeRatio],
    ) -> eyre::Result<Self> {
        let mut sum = 0;
        let pool_sizes = ratios
            .iter()
            .map(|r| {
                let size = vk::DescriptorPoolSize::default()
                    .ty(r.descriptor_type)
                    .descriptor_count((r.ratio * max_sets as f32) as u32);
                sum += size.descriptor_count;
                size
            })
            .collect::<Vec<_>>();
        debug_assert_eq!(sum, max_sets);

        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(max_sets)
            .pool_sizes(&pool_sizes);
        let pool = unsafe { device.create_descriptor_pool(&info, None) }?;
        Ok(Self { pool })
    }
    pub fn clear_descriptors(
        &self,
        device: &ash::Device,
    ) -> std::result::Result<(), ash::vk::Result> {
        unsafe { device.reset_descriptor_pool(self.pool, vk::DescriptorPoolResetFlags::empty()) }
    }
    pub fn destroy_pool(&self, device: &ash::Device) {
        unsafe { device.destroy_descriptor_pool(self.pool, None) };
    }
    pub fn allocate(
        &self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
    ) -> Result<Vec<vk::DescriptorSet>, vk::Result> {
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);

        unsafe { device.allocate_descriptor_sets(&info) }
    }
}
