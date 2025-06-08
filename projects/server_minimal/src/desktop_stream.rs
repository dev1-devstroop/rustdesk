use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::time::{Duration, Instant};
use anyhow::Result;

pub struct DesktopStreamer {
    capturer: Capturer,
    frame_rate: u32,
    last_frame: Instant,
}

impl DesktopStreamer {
    pub fn new(screen_id: u32, frame_rate: u32) -> Result<Self> {
        let display = Display::primary().map_err(|e| anyhow::anyhow!("Failed to get primary display: {}", e))?;
        let capturer = Capturer::new(display).map_err(|e| anyhow::anyhow!("Failed to create capturer: {}", e))?;
        
        Ok(Self {
            capturer,
            frame_rate,
            last_frame: Instant::now(),
        })
    }

    pub fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let frame_duration = Duration::from_millis(1000 / self.frame_rate as u64);
        
        if self.last_frame.elapsed() < frame_duration {
            return Ok(None);
        }

        match self.capturer.frame() {
            Ok(frame) => {
                self.last_frame = Instant::now();
                
                // Convert BGRA to RGB and compress (simple implementation)
                let rgb_data = self.bgra_to_rgb(&frame);
                Ok(Some(rgb_data))
            }
            Err(error) => {
                if error.kind() == WouldBlock {
                    // Frame not ready yet
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("Capture error: {}", error))
                }
            }
        }
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        (self.capturer.width() as u32, self.capturer.height() as u32)
    }

    fn bgra_to_rgb(&self, bgra_data: &[u8]) -> Vec<u8> {
        let mut rgb_data = Vec::with_capacity((bgra_data.len() / 4) * 3);
        
        for chunk in bgra_data.chunks_exact(4) {
            rgb_data.push(chunk[2]); // R
            rgb_data.push(chunk[1]); // G
            rgb_data.push(chunk[0]); // B
        }
        
        rgb_data
    }
}
