use std::{
    borrow::Cow,
    ffi::{self, CStr},
};

use ash::vk::{self};
use eyre::{Context, ContextCompat};
use winit::{
    raw_window_handle::{DisplayHandle, HasDisplayHandle, HasWindowHandle},
    window::Window,
};

pub struct Vulkan {
    entry: ash::Entry,
    instance: ash::Instance,
    debug_messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    surface: vk::SurfaceKHR,
    graphics_queue_index: u32,
    graphics_queue: vk::Queue,
}

const VALIDATION_ENABLED: bool = cfg!(debug_assertions);

/// The Vulkan SDK version that started requiring the portability subset extension for macOS.
pub const PORTABILITY_MACOS_VERSION: u32 = vk::make_api_version(0, 1, 3, 216);

fn build_instance(
    entry: &ash::Entry,
    display_handle: DisplayHandle,
    name: &CStr,
    version: u32,
    use_validation: bool,
) -> eyre::Result<ash::Instance> {
    let app_info = vk::ApplicationInfo::default()
        .application_name(name)
        .api_version(version);

    let layers = if use_validation {
        vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
    } else {
        vec![]
    };

    let mut extension_names =
        ash_window::enumerate_required_extensions(display_handle.as_raw())?.to_vec();
    extension_names.push(vk::EXT_DEBUG_UTILS_NAME.as_ptr());
    extension_names.push(ash::khr::surface::NAME.as_ptr());
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
        extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
    }
    let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };
    let info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extension_names)
        .flags(create_flags);

    unsafe { entry.create_instance(&info, None) }.wrap_err("could not create instance")
}
fn build_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> eyre::Result<vk::DebugUtilsMessengerEXT> {
    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback));

    let debug_utils_loader = ash::ext::debug_utils::Instance::new(entry, instance);

    unsafe {
        debug_utils_loader
            .create_debug_utils_messenger(&debug_info, None)
            .wrap_err("could not create debug messenger")
    }
}

const fn get_api(api: u32) -> (u32, u32, u32, u32) {
    let variant = vk::api_version_variant(api);
    let major = vk::api_version_major(api);
    let minor = vk::api_version_minor(api);
    let patch = vk::api_version_patch(api);
    (variant, major, minor, patch)
}

const DEVICE_EXTENSION_NAMES: &[*const i8] = &[
    ash::khr::swapchain::NAME.as_ptr(),
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    ash::khr::portability_subset::NAME.as_ptr(),
];
fn select_physical_device_and_graphics_queue(
    entry: &ash::Entry,
    instance: &ash::Instance,
    surface: vk::SurfaceKHR,
    minimum_api_version: u32,
) -> eyre::Result<(vk::PhysicalDevice, u32)> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .wrap_err("could not enumerate physical devices")?;

    let surface_loader = ash::khr::surface::Instance::new(entry, instance);
    let (physical_device, graphics_queue_index) = physical_devices
        .iter()
        .find_map(|pd| {
            let props = unsafe { instance.get_physical_device_queue_family_properties(*pd) };
            let index = props.iter().enumerate().find_map(|(index, prop)| {
                let support_graphics = prop.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                let support_surface = unsafe {
                    surface_loader.get_physical_device_surface_support(*pd, index as u32, surface)
                }
                .ok()?;
                (support_graphics && support_surface).then_some(index)
            })?;

            let props = unsafe { instance.get_physical_device_properties(*pd) };
            let api_supported = props.api_version >= minimum_api_version;
            let is_discrete = props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU;
            let mut features_13 = vk::PhysicalDeviceVulkan13Features::default();
            let mut features_12 = vk::PhysicalDeviceVulkan12Features {
                p_next: (&raw mut features_13).cast(),
                ..Default::default()
            };
            let mut features2 = vk::PhysicalDeviceFeatures2 {
                p_next: (&raw mut features_12).cast(),
                ..Default::default()
            };
            unsafe { instance.get_physical_device_features2(*pd, &mut features2) };
            let b_true = true.into();
            let has_features = features_13.dynamic_rendering == b_true
                && features_13.synchronization2 == b_true
                && features_12.buffer_device_address == b_true
                && features_12.descriptor_indexing == b_true;

            (api_supported && has_features && is_discrete).then_some((pd, index))
        })
        .wrap_err("could not find suitable devices")?;
    Ok((*physical_device, graphics_queue_index as u32))
}

fn build_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue: u32,
) -> eyre::Result<ash::Device> {
    let mut features_13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true);
    let mut features_12 = vk::PhysicalDeviceVulkan12Features::default()
        .buffer_device_address(true)
        .descriptor_indexing(true);
    features_12.p_next = (&raw mut features_13).cast();

    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue)
        .queue_priorities(&[1.0]);
    let queue_infos = [queue_info];
    let features = vk::PhysicalDeviceFeatures::default();
    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(DEVICE_EXTENSION_NAMES)
        .enabled_features(&features)
        .push_next(&mut features_12);
    unsafe { instance.create_device(physical_device, &device_info, None) }
        .wrap_err("could not create device")
}
impl Vulkan {
    pub fn new(window: &Window) -> eyre::Result<Self> {
        let entry = unsafe { ash::Entry::load() }?;
        let display_handle = window.display_handle().wrap_err("window handle error")?;
        let window_handle = window.window_handle().wrap_err("window handle error")?;
        let api_version = vk::make_api_version(0, 1, 3, 0);
        let instance = build_instance(
            &entry,
            display_handle,
            c"Vulkan Example",
            api_version,
            cfg!(debug_assertions),
        )?;
        let debug_messenger = build_messenger(&entry, &instance)?;

        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                display_handle.as_raw(),
                window_handle.as_raw(),
                None,
            )
            .wrap_err("could not create surface")?
        };

        let (physical_device, graphics_queue_index) =
            select_physical_device_and_graphics_queue(&entry, &instance, surface, api_version)?;
        let device = build_device(&instance, physical_device, graphics_queue_index)?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };

        Ok(Self {
            entry,
            instance,
            debug_messenger,
            physical_device,
            device,
            surface,
            graphics_queue_index,
            graphics_queue,
        })
    }

    pub const fn instance(&self) -> &ash::Instance {
        &self.instance
    }
    pub fn surface_instance(&self) -> ash::khr::surface::Instance {
        ash::khr::surface::Instance::new(&self.entry, &self.instance)
    }
    pub fn debug_instance(&self) -> ash::ext::debug_utils::Instance {
        ash::ext::debug_utils::Instance::new(&self.entry, &self.instance)
    }

    pub const fn device(&self) -> &ash::Device {
        &self.device
    }
    pub fn swapchain_device(&self) -> ash::khr::swapchain::Device {
        ash::khr::swapchain::Device::new(&self.instance, &self.device)
    }

    pub const fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub const fn surface(&self) -> vk::SurfaceKHR {
        self.surface
    }

    pub const fn debug_messenger(&self) -> vk::DebugUtilsMessengerEXT {
        self.debug_messenger
    }

    pub const fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    pub fn graphics_queue_index(&self) -> u32 {
        self.graphics_queue_index
    }

    pub fn graphics_queue(&self) -> vk::Queue {
        self.graphics_queue
    }
}
extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = unsafe { *p_callback_data };
    let id_number = callback_data.message_id_number;

    let id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        unsafe { ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy() }
    };
    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        unsafe { ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy() }
    };
    let format = format!("{severity:?}:\n{message_type:?} [{id_name} ({id_number})] : {message}");
    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        tracing::error!("{format}");
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
        tracing::warn!("{format}");
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
        tracing::debug!("{format}");
    } else {
        tracing::trace!("{format}");
    }

    vk::FALSE
}
