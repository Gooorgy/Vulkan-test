use crate::vulkan_render::buffer::AllocatedBuffer;
use crate::vulkan_render::constants::MAX_FRAMES_IN_FLIGHT;
use crate::vulkan_render::descriptor::DescriptorManager;
use crate::vulkan_render::device::DeviceInfo;
use crate::vulkan_render::graphics_pipeline::PipelineInfo;
use crate::vulkan_render::image_util::AllocatedImage;
use crate::vulkan_render::structs::{CameraMvpUbo, LightingUbo, ModelDynamicUbo};
use ash::vk::{
    BufferUsageFlags, DescriptorSet, Extent2D, Format, ImageAspectFlags, ImageView,
    MemoryPropertyFlags, Sampler,
};
use ash::{vk, Instance};
use glm::{normalize, vec3, vec3_to_vec4, vec4};
use std::mem;
use crate::vulkan_render::utils;
use crate::vulkan_render::utils::get_buffer_alignment;

#[allow(dead_code)]
pub struct FrameData {
    pub render_semaphore: vk::Semaphore,
    pub swapchain_semaphore: vk::Semaphore,
    pub render_fence: vk::Fence,
    pub command_buffer: vk::CommandBuffer,

    pub camera_mvp_buffer: AllocatedBuffer,
    pub model_dynamic_buffer: AllocatedBuffer,
    pub lighting_buffer: AllocatedBuffer,

    pub descriptor_gbuffer_set: DescriptorSet,
    pub descriptor_lighting_set: DescriptorSet,

    pub albedo_image: AllocatedImage,
    pub albedo_sampler: Sampler,

    pub normal_image: AllocatedImage,
    pub normal_sampler: Sampler,

    pub depth_image: AllocatedImage,
    pub depth_sampler: Sampler,

    pub shadow_map_image: AllocatedImage,
    pub shadow_map_sampler: Sampler,

    pub draw_image: AllocatedImage,
}

impl FrameData {
    pub fn update_camera_mvp_buffer(&mut self, mvp: CameraMvpUbo) {
        self.camera_mvp_buffer.update_buffer(&[mvp]);
    }

    pub fn update_model_dynamic_buffer(&mut self, mvp: Vec<ModelDynamicUbo>) {
        self.model_dynamic_buffer.update_buffer(&mvp);
    }

    #[allow(dead_code)]
    pub fn update_lighting_buffer(&mut self, mvp: LightingUbo) {
        self.lighting_buffer.update_buffer(&[mvp]);
    }
}

pub struct FrameManager {
    frames: Vec<FrameData>,
    current_frame: usize,
    frame_count: usize,
    _descriptor_manager: DescriptorManager,
    pub gbuffer_pipeline: PipelineInfo,
    pub lighting_pipeline: PipelineInfo,
    pub model_ubo_alignment: u64
}

impl FrameManager {
    pub fn new(
        device_info: &DeviceInfo,
        instance: &Instance,
        max_frames: usize,
        extent2d: Extent2D,
        mesh_count: usize,
        texture_sampler: &Sampler,
        texture_image_view: &ImageView,
    ) -> Self {
        let image_width = extent2d.width;
        let image_height = extent2d.height;
        let command_buffers = Self::create_command_buffers(device_info);
        let descriptor_manager = DescriptorManager::new(&device_info.logical_device, max_frames);

        let mut frame_data = vec![];
        let pipeline = PipelineInfo::new_gbuffer_pipeline(
            &device_info.logical_device,
            &descriptor_manager.global_gbuffer_layout,
        );
        let lighting_pipeline = PipelineInfo::new_lighing_pipeline(
            &device_info.logical_device,
            &descriptor_manager.global_lighting_layout,
        );

        let model_ubo_alignment = get_buffer_alignment::<ModelDynamicUbo>(device_info);

        for frame in 0..max_frames {
            let command_buffer = command_buffers[frame];
            let (swapchain_semaphore, render_semaphore, render_fence) =
                Self::create_sync_objects(&device_info.logical_device);

            let camera_mvp_buffer = Self::create_camera_mvp_buffer(device_info, instance);
            let model_dynamic_buffer =
                Self::create_model_dynamic_uniform_buffer(device_info, instance, mesh_count, model_ubo_alignment);
            let lighting_buffer = Self::create_lighting_buffer(device_info, instance);

            let (albedo_image, normal_image, depth_image, shadow_map_image, draw_image) =
                Self::create_images(device_info, instance, image_width, image_height);

            let albedo_sampler = utils::create_texture_sampler(device_info, instance);
            let normal_sampler = utils::create_texture_sampler(device_info, instance);
            let depth_sampler = utils::create_texture_sampler(device_info, instance);
            let shadow_map_sampler = utils::create_texture_sampler(device_info, instance);

            let gbuffer_descriptor_set =
                descriptor_manager.create_gbuffer_descriptor_set(&device_info.logical_device);
            descriptor_manager.update_gbuffer_descriptor_set(
                device_info,
                &camera_mvp_buffer,
                &model_dynamic_buffer,
                model_ubo_alignment,
                texture_image_view,
                texture_sampler,
                gbuffer_descriptor_set,
            );

            let lighting_descriptor_set =
                descriptor_manager.create_lighting_descriptor_set(&device_info.logical_device);
            descriptor_manager.update_lighting_descriptor_set(
                device_info,
                &lighting_buffer,
                &albedo_image.image_view,
                &albedo_sampler,
                &normal_image.image_view,
                &normal_sampler,
                &depth_image.image_view,
                &depth_sampler,
                lighting_descriptor_set,
            );

            frame_data.push(FrameData {
                render_semaphore,
                swapchain_semaphore,
                render_fence,
                command_buffer,
                camera_mvp_buffer,
                model_dynamic_buffer,
                lighting_buffer,
                descriptor_gbuffer_set: gbuffer_descriptor_set,
                descriptor_lighting_set: lighting_descriptor_set,
                albedo_image,
                albedo_sampler,
                normal_image,
                normal_sampler,
                depth_image,
                depth_sampler,
                shadow_map_image,
                shadow_map_sampler,
                draw_image,
            });
        }

        Self {
            _descriptor_manager: descriptor_manager,
            frames: frame_data,
            current_frame: 0,
            frame_count: max_frames,
            gbuffer_pipeline: pipeline,
            lighting_pipeline,
            model_ubo_alignment
        }
    }

    pub fn advance_frame(&mut self) {
        self.current_frame = (self.current_frame + 1) % self.frame_count;
    }

    pub fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.current_frame]
    }

    pub fn get_mut_current_frame(&mut self) -> &mut FrameData {
        self.frames.get_mut(self.current_frame).unwrap()
    }

    fn create_command_buffers(device_info: &DeviceInfo) -> Vec<vk::CommandBuffer> {
        let command_buffer_alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(device_info.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(MAX_FRAMES_IN_FLIGHT);

        unsafe {
            device_info
                .logical_device
                .allocate_command_buffers(&command_buffer_alloc_info)
                .expect("failed to allocate command buffer")
        }
    }

    fn create_camera_mvp_buffer(device_info: &DeviceInfo, instance: &Instance) -> AllocatedBuffer {
        let buffer_size = mem::size_of::<CameraMvpUbo>() as u64;
        AllocatedBuffer::new(
            device_info,
            instance,
            buffer_size,
            BufferUsageFlags::UNIFORM_BUFFER,
            MemoryPropertyFlags::HOST_VISIBLE | MemoryPropertyFlags::HOST_COHERENT,
        )
    }

    fn create_lighting_buffer(device_info: &DeviceInfo, instance: &Instance) -> AllocatedBuffer {
        let buffer_size = mem::size_of::<LightingUbo>() as u64;
        let mut buffer = AllocatedBuffer::new(
            device_info,
            instance,
            buffer_size,
            BufferUsageFlags::UNIFORM_BUFFER,
            MemoryPropertyFlags::HOST_VISIBLE | MemoryPropertyFlags::HOST_COHERENT,
        );

        let light_dir = normalize(&vec3(-1.0, -1.0, -1.0));

        buffer.update_buffer(&[LightingUbo {
            light_direction: vec3_to_vec4(&light_dir),

            // w is intensity
            light_color: vec4(1.0, 1.0, 0.0, 2.0),
            ambient_light: vec4(0.1, 0.1, 0.1, 0.2),
        }]);

        buffer
    }

    fn create_model_dynamic_uniform_buffer(
        device_info: &DeviceInfo,
        instance: &Instance,
        mesh_count: usize,
        dynamic_alignment: u64,
    ) -> AllocatedBuffer {
        let dynamic_buffer_size = dynamic_alignment * mesh_count as u64;

        let buffer = AllocatedBuffer::new(
            device_info,
            instance,
            dynamic_buffer_size,
            BufferUsageFlags::UNIFORM_BUFFER,
            MemoryPropertyFlags::HOST_VISIBLE,
        );

        buffer
    }

    //noinspection DuplicatedCode
    fn create_images(
        device_info: &DeviceInfo,
        instance: &Instance,
        image_width: u32,
        image_height: u32,
    ) -> (
        AllocatedImage,
        AllocatedImage,
        AllocatedImage,
        AllocatedImage,
        AllocatedImage,
    ) {
        let albedo_image = AllocatedImage::new(
            device_info,
            instance,
            image_width,
            image_height,
            Format::R16G16B16A16_SFLOAT,
            ImageAspectFlags::COLOR,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
            MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let normal_image = AllocatedImage::new(
            device_info,
            instance,
            image_width,
            image_height,
            Format::R16G16B16A16_SNORM,
            ImageAspectFlags::COLOR,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
            MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let depth_image = AllocatedImage::new(
            device_info,
            instance,
            image_width,
            image_height,
            Format::D32_SFLOAT,
            ImageAspectFlags::DEPTH,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
            MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let shadow_map_image = AllocatedImage::new(
            device_info,
            instance,
            2048,
            2048,
            Format::D32_SFLOAT,
            ImageAspectFlags::DEPTH,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let draw_image = AllocatedImage::new(
            device_info,
            instance,
            image_width,
            image_height,
            Format::R16G16B16A16_SFLOAT,
            ImageAspectFlags::COLOR,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            MemoryPropertyFlags::DEVICE_LOCAL,
        );

        (
            albedo_image,
            normal_image,
            depth_image,
            shadow_map_image,
            draw_image,
        )
    }

    fn create_sync_objects(device: &ash::Device) -> (vk::Semaphore, vk::Semaphore, vk::Fence) {
        let semaphore_create_info = vk::SemaphoreCreateInfo {
            s_type: vk::StructureType::SEMAPHORE_CREATE_INFO,
            ..Default::default()
        };

        let fence_create_info = vk::FenceCreateInfo {
            s_type: vk::StructureType::FENCE_CREATE_INFO,
            flags: vk::FenceCreateFlags::SIGNALED,
            ..Default::default()
        };

        unsafe {
            let swapchain_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("failed to create semaphore for swapchain");

            let render_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("failed to create semaphore for render semaphore");

            let render_fence = device
                .create_fence(&fence_create_info, None)
                .expect("failed to create fence for render fence");

            (swapchain_semaphore, render_semaphore, render_fence)
        }
    }
}