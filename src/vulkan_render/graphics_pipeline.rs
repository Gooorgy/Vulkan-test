use std::{ffi::CString, fs, io, path::Path, ptr};

use ash::vk;

use super::structs::Vertex;

const FRAGMENT_SHADER: &str = "frag";
const VERTEX_SHADER: &str = "vert";
const SHADER_PATH: &str = ".\\src\\shaders";
const SHADER_EXTENSION: &str = ".spv";

pub struct PipelineInfo {
    pub graphics_pipelines: Vec<vk::Pipeline>,
    _pipeline_layout: vk::PipelineLayout,
}

impl PipelineInfo {
    pub fn new(render_pass: &vk::RenderPass, logical_device: &ash::Device) -> PipelineInfo {
        let vert_shader_code =
            Self::read_shader_file(VERTEX_SHADER).expect("Unable to read vertex file");
        let frag_shader_code =
            Self::read_shader_file(FRAGMENT_SHADER).expect("Unable to read fragment shader");

        let vert_shader_module = Self::create_shader_module(&vert_shader_code, logical_device);
        let frag_shader_module = Self::create_shader_module(&frag_shader_code, logical_device);

        let shader_name = CString::new("main").unwrap();

        let vert_shader_stage_create_info = ash::vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_shader_module)
            .name(&shader_name);

        let frag_shader_stage_create_info = ash::vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_shader_module)
            .name(&shader_name);

        let shader_stages = [vert_shader_stage_create_info, frag_shader_stage_create_info];

        let dynamic_states = [
            ash::vk::DynamicState::VIEWPORT,
            ash::vk::DynamicState::SCISSOR,
        ];

        let dynamic_state_create_info =
            ash::vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let vertex_binding_description = Vertex::get_binding_descriptions();
        let vertex_attribute_description = Vertex::get_attribute_descriptions();

        let vertex_input_info_create_info = ash::vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&vertex_attribute_description)
            .vertex_binding_descriptions(&vertex_binding_description);

        let input_assembly_create_info = ash::vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(true);

        let viewport_state_create_info = ash::vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer_create_info = ash::vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .depth_bias_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0_f32)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::CLOCKWISE);

        let multisampling_create_info = ash::vk::PipelineMultisampleStateCreateInfo {
            s_type: ash::vk::StructureType::PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
            sample_shading_enable: ash::vk::FALSE,
            rasterization_samples: ash::vk::SampleCountFlags::TYPE_1,
            min_sample_shading: 1.0,
            p_sample_mask: ptr::null(),
            alpha_to_coverage_enable: ash::vk::FALSE,
            alpha_to_one_enable: ash::vk::FALSE,
            ..Default::default()
        };

        let color_blend_attachment = ash::vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);

        let color_blend_attachments = [color_blend_attachment];
        let color_blending_create_info = ash::vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(true)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments);

        let pipeline_layout_create_info = ash::vk::PipelineLayoutCreateInfo::default();

        let pipeline_layout = unsafe {
            logical_device
                .create_pipeline_layout(&pipeline_layout_create_info, None)
                .expect("Unable to create pipeline layout")
        };

        let pipeline_create_info = ash::vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info_create_info)
            .input_assembly_state(&input_assembly_create_info)
            .viewport_state(&viewport_state_create_info)
            .rasterization_state(&rasterizer_create_info)
            .multisample_state(&multisampling_create_info)
            .color_blend_state(&color_blending_create_info)
            .dynamic_state(&dynamic_state_create_info)
            .layout(pipeline_layout)
            .render_pass(*render_pass)
            .subpass(0)
            .base_pipeline_handle(vk::Pipeline::null())
            .base_pipeline_index(-1);

        let graphics_pipelines = unsafe {
            logical_device
                .create_graphics_pipelines(
                    ash::vk::PipelineCache::null(),
                    &[pipeline_create_info],
                    None,
                )
                .expect("Unable to create graphics pipeline")
        };

        unsafe {
            logical_device.destroy_shader_module(vert_shader_module, None);
            logical_device.destroy_shader_module(frag_shader_module, None);
        };

        Self {
            graphics_pipelines,
            _pipeline_layout: pipeline_layout,
        }
    }

    fn read_shader_file(shader_name: &str) -> Result<Vec<u8>, io::Error> {
        let path = Path::new(SHADER_PATH).join(format!("{}{}", shader_name, SHADER_EXTENSION));

        println!("{:?}", path);
        fs::read(path)
    }

    fn create_shader_module(code: &[u8], device: &ash::Device) -> ash::vk::ShaderModule {
        unsafe {
            let (_prefix, shorts, _suffix) = code.align_to::<u32>();
            let create_info = ash::vk::ShaderModuleCreateInfo::default().code(shorts);
            device
                .create_shader_module(&create_info, None)
                .expect("Unable to create shader module")
        }
    }
}
