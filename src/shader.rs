use std::{
    fs::{self, read_to_string},
    path::Path,
};

use ash::vk;
use eyre::{Context, OptionExt};
use shaderc::ResolvedInclude;

pub struct ShaderCompiler {
    compiler: shaderc::Compiler,
}

impl std::ops::Deref for ShaderCompiler {
    type Target = shaderc::Compiler;

    fn deref(&self) -> &Self::Target {
        &self.compiler
    }
}

impl ShaderCompiler {
    pub fn new() -> eyre::Result<Self> {
        let compiler = shaderc::Compiler::new()?;

        Ok(Self { compiler })
    }
    pub fn default_options<'a>() -> shaderc::Result<shaderc::CompileOptions<'a>> {
        let mut options = shaderc::CompileOptions::new()?;
        options.set_target_env(
            shaderc::TargetEnv::Vulkan,
            shaderc::EnvVersion::Vulkan1_3 as u32,
        );
        options.set_include_callback(|requested, include_type, requestee, depth| {
            let files = std::fs::read_dir(Path::new("shaders"))
                .map_err(|e| format!("could not open shaders dir {e}"))?;
            if let Some(requested_file) = files.flatten().find(|f| f.file_name() == requested) {
                let path = requested_file.path();
                let abs_path = std::fs::canonicalize(&path)
                    .map_err(|e| format!("could not canonicalize path {e}"))?;
                let resolved_name = abs_path.as_os_str().to_string_lossy().to_string();

                let content =
                    fs::read_to_string(path).map_err(|e| format!("could not read content{e}"))?;
                Ok(ResolvedInclude {
                    resolved_name,
                    content,
                })
            } else {
                Err("could not find requested file".to_owned())
            }
        });
        Ok(options)
    }
    pub fn create_shader_module_from_path(
        &self,
        device: &ash::Device,
        path: impl AsRef<Path>,
        kind: shaderc::ShaderKind,
        entry_point: &str,
    ) -> eyre::Result<vk::ShaderModule> {
        let spv = self.compile_from_path(path, kind, entry_point)?;
        let info = vk::ShaderModuleCreateInfo::default().code(spv.as_binary());
        unsafe {
            device
                .create_shader_module(&info, None)
                .wrap_err("could not create shader module")
        }
    }
    pub fn create_shader_module_from_str(
        &self,
        device: &ash::Device,
        source: &str,
        kind: shaderc::ShaderKind,
        file_name: &str,
        entry_point: &str,
    ) -> eyre::Result<vk::ShaderModule> {
        let spv = self.compile_from_str(source, kind, file_name, entry_point)?;
        let info = vk::ShaderModuleCreateInfo::default().code(spv.as_binary());
        unsafe {
            device
                .create_shader_module(&info, None)
                .wrap_err("could not create shader module")
        }
    }

    pub fn compile_from_str(
        &self,
        source: &str,
        kind: shaderc::ShaderKind,
        file_name: &str,
        entry_point: &str,
    ) -> shaderc::Result<shaderc::CompilationArtifact> {
        let options = Self::default_options()?;
        self.compiler
            .compile_into_spirv(source, kind, file_name, entry_point, Some(&options))
    }
    pub fn compile_from_path(
        &self,
        path: impl AsRef<Path>,
        kind: shaderc::ShaderKind,
        entry_point: &str,
    ) -> eyre::Result<shaderc::CompilationArtifact> {
        let file_name = path
            .as_ref()
            .file_name()
            .ok_or_eyre("could not get filename")?
            .to_string_lossy();
        let source = read_to_string(&path)?;
        self.compile_from_str(&source, kind, &file_name, entry_point)
            .wrap_err("could not compile shader")
    }
}
