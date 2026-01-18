use ash::vk;

pub struct DescriptorLayoutBuilder<'a> {
    bindings: Vec<vk::DescriptorSetLayoutBinding<'a>>,
}

impl DescriptorLayoutBuilder<'_> {
    pub const fn new() -> Self {
        Self { bindings: vec![] }
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
    pub fn clear(&mut self) {
        self.bindings.clear();
    }
    pub fn build(
        mut self,
        device: &ash::Device,
        shader_stage: vk::ShaderStageFlags,
        // flags: vk::DescriptorSetLayoutCreateFlags,
        // next: Option<&mut T>,
    ) -> Result<vk::DescriptorSetLayout, vk::Result>
// where
    //     T: vk::ExtendsDescriptorSetLayoutCreateInfo + ?Sized,
    {
        for b in &mut self.bindings {
            b.stage_flags |= shader_stage;
        }

        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&self.bindings);
        // .flags(flags);
        // let info = match next {
        //     Some(next) => info.push_next(next),
        //     None => info,
        // };

        unsafe { device.create_descriptor_set_layout(&info, None) }
    }
}

impl Default for DescriptorLayoutBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}
pub struct PoolSizeRatio {
    descriptor_type: vk::DescriptorType,
    ratio: f32,
}

impl PoolSizeRatio {
    pub const fn new(descriptor_type: vk::DescriptorType, ratio: f32) -> Self {
        debug_assert!(ratio >= 0.0 && ratio <= 1.0);
        Self {
            descriptor_type,
            ratio,
        }
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
        let mut pool_sizes = Vec::with_capacity(ratios.len());
        let mut sum = 0;
        for ratio in ratios {
            let pool_size = vk::DescriptorPoolSize::default()
                .ty(ratio.descriptor_type)
                .descriptor_count((ratio.ratio * max_sets as f32) as u32);
            sum += pool_size.descriptor_count;
            pool_sizes.push(pool_size);
        }
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
