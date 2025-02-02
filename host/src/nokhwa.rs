use super::wasi;
use std::sync::Arc;

use wasi::sensor::property::PropertyKey;
use wasi::sensor::property::PropertyValue;
use wasi::sensor::sensor::DeviceError;

use nokhwa::error::NokhwaError;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::CameraIndex;
use nokhwa::utils::FrameFormat;
use nokhwa::utils::RequestedFormat;
use nokhwa::utils::RequestedFormatType;
use nokhwa::CallbackCamera;

use crate::traits::BufferPool;
use crate::traits::SensorDevice;

pub struct NokhwaDevice {
    camera: CallbackCamera,
}

impl NokhwaDevice {
    pub fn new() -> Result<Self, DeviceError> {
        nokhwa::nokhwa_initialize(|granted| {
            println!("granted: {}", granted);
        });
        println!("NokhwaDevice granted");
        let index = CameraIndex::Index(0); // XXX should not hardcode
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        println!("NokhwaDevice creating a threaded camera");
        let mut camera = CallbackCamera::new(index, requested, |buffer| {
            println!("NokhwaDevice dummy callback (this should not be called)");
        })?;
        Ok(Self { camera: camera })
    }
}

impl From<NokhwaError> for DeviceError {
    fn from(error: NokhwaError) -> Self {
        println!("converting nokhwa error: {}", error);
        DeviceError::Unknown // XXX do a more appropriate conversion
    }
}

impl NokhwaDevice {
    fn set_frame_rate(&mut self, frame_rate: f32) -> Result<(), DeviceError> {
        println!("NokhwaDevice set_frame_rate {}", frame_rate);
        if frame_rate < 1.0 {
            return Err(DeviceError::NotSupported);
        }
        let frame_rate = frame_rate as u32;
        self.camera.set_frame_rate(frame_rate)?;
        Ok(())
    }
    fn get_frame_rate(&mut self) -> Result<f32, DeviceError> {
        let frame_rate = self.camera.frame_rate()?;
        Ok(frame_rate as f32)
    }
}

impl SensorDevice for NokhwaDevice {
    fn start_streaming(
        &mut self,
        pool: Arc<dyn BufferPool + Send + Sync>,
    ) -> Result<(), DeviceError> {
        let ref mut camera = self.camera;
        camera.set_callback(move |buffer| {
            println!("NokhwaDevice callback");
            let resolution = buffer.resolution();
            let width = resolution.width();
            let height = resolution.height();
            let (pixel_format, channels) = match buffer.source_frame_format() {
                FrameFormat::YUYV => (wasi::buffer_pool::data_types::PixelFormat::Yuy2, 4),
                _ => {
                    println!("NokhwaDevice dropping a frame with unimplemented format");
                    return;
                }
            };
            let image = wasi::buffer_pool::data_types::Image {
                dimension: wasi::buffer_pool::data_types::Dimension {
                    width,
                    height,
                    stride_bytes: width * channels,
                    pixel_format: pixel_format,
                },
                payload: buffer.buffer().to_vec(),
            };
            let data = wasi::buffer_pool::buffer_pool::FrameData::ByValue(
                wasi::buffer_pool::data_types::DataType::Image(image),
            );
            match pool.enqueue(Box::new(data), None) {
                Ok(_) => println!("NokhwaDevice frame enqueued"),
                _ => println!("NokhwaDevice frame dropped"),
            }
        })?;
        println!("NokhwaDevice calling open_stream");
        camera.open_stream()?;
        println!("NokhwaDevice started");
        Ok(())
    }
    fn stop_streaming(&mut self) -> Result<(), DeviceError> {
        self.camera.stop_stream()?;
        Ok(())
    }
    fn set_property(&mut self, key: PropertyKey, value: PropertyValue) -> Result<(), DeviceError> {
        match key {
            PropertyKey::SamplingRate => {
                let PropertyValue::Fraction(frac) = value else {
                    return Err(DeviceError::InvalidArgument);
                };
                if frac.numerator == 0 || frac.denominator == 0 {
                    return Err(DeviceError::InvalidArgument);
                }
                let frame_rate = frac.numerator as f32 / frac.denominator as f32;
                self.set_frame_rate(frame_rate)?;
            }
            _ => return Err(DeviceError::NotSupported),
        };
        Ok(())
    }
    fn get_property(&mut self, key: PropertyKey) -> Result<PropertyValue, DeviceError> {
        let value = match key {
            PropertyKey::SamplingRate => {
                let frame_rate = self.get_frame_rate()?;
                // note: nokhwa support only u32 frame rate.
                PropertyValue::Fraction(wasi::sensor::property::Fraction {
                    numerator: frame_rate as u32,
                    denominator: 1,
                })
            }
            _ => return Err(DeviceError::NotSupported),
        };
        Ok(value)
    }
}
