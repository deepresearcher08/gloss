use std::collections::HashMap;

use gloss_text::{CacheKey, GlyphBitmap, GlyphContent};

pub(crate) const ATLAS_SIZE: u32 = 2048;

#[derive(Clone, Debug)]
struct GlyphEntry {
    uv: [f32; 4],
    width: u32,
    height: u32,
    last_used: u64,
    coverage: Vec<u8>,
}

/// A shelf-packed R8 glyph atlas with LRU eviction.
pub(crate) struct Atlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    entries: HashMap<CacheKey, GlyphEntry>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    frame: u64,
    needs_repack: bool,
    generation: u64,
}

impl Atlas {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let (texture, view) = create_texture(device);
        Self {
            texture,
            view,
            entries: HashMap::new(),
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
            frame: 0,
            needs_repack: false,
            generation: 0,
        }
    }

    /// Advances the frame counter and repacks the atlas when it overflowed.
    ///
    /// Repacking outside the glyph insertion path avoids invalidating UVs that
    /// were already queued for the current frame.
    pub(crate) fn begin_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame = self.frame.wrapping_add(1);
        if self.needs_repack {
            self.evict_and_repack(device, queue);
            self.needs_repack = false;
        }
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Bumped whenever the backing texture view is replaced.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Fraction of atlas area currently occupied by glyphs.
    pub(crate) fn occupancy(&self) -> f32 {
        let used: u64 = self
            .entries
            .values()
            .map(|entry| u64::from(entry.width) * u64::from(entry.height))
            .sum();
        used as f32 / (u64::from(ATLAS_SIZE) * u64::from(ATLAS_SIZE)) as f32
    }

    /// Number of cached glyphs.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn insert(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        bitmap: &GlyphBitmap,
    ) -> Option<[f32; 4]> {
        if let Some(entry) = self.entries.get_mut(&bitmap.cache_key) {
            entry.last_used = self.frame;
            return Some(entry.uv);
        }
        let width = bitmap.width;
        let height = bitmap.height;
        if width == 0 || height == 0 || width > ATLAS_SIZE || height > ATLAS_SIZE {
            return None;
        }

        let (x, y) = match self.allocate(width, height) {
            (Some(x), Some(y)) => (x, y),
            _ => {
                self.needs_repack = true;
                return None;
            }
        };
        self.cursor_x = x + width + 1;
        self.row_height = self.row_height.max(height);

        let coverage = match bitmap.content {
            GlyphContent::Mask => bitmap.data.clone(),
            GlyphContent::Color => bitmap.data.chunks_exact(4).map(|pixel| pixel[3]).collect(),
        };
        if coverage.len() < (width * height) as usize {
            return None;
        }

        upload(&self.texture, queue, x, y, width, height, &coverage);
        let uv = uv_for(x, y, width, height);
        self.entries.insert(
            bitmap.cache_key,
            GlyphEntry {
                uv,
                width,
                height,
                last_used: self.frame,
                coverage,
            },
        );
        Some(uv)
    }

    fn allocate(&mut self, width: u32, height: u32) -> (Option<u32>, Option<u32>) {
        if self.cursor_x + width + 1 > ATLAS_SIZE {
            self.cursor_x = 1;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.cursor_y + height + 1 > ATLAS_SIZE {
            return (None, None);
        }
        (Some(self.cursor_x), Some(self.cursor_y))
    }

    fn evict_and_repack(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.entries.is_empty() {
            self.cursor_x = 1;
            self.cursor_y = 1;
            self.row_height = 0;
            return;
        }

        let mut kept: Vec<(CacheKey, GlyphEntry)> = self.entries.drain().collect();
        kept.sort_by_key(|(_, entry)| entry.last_used);
        let evict = (kept.len() / 4).max(1).min(kept.len());
        kept.drain(..evict);

        let (texture, view) = create_texture(device);
        self.texture = texture;
        self.view = view;
        self.generation = self.generation.wrapping_add(1);
        self.cursor_x = 1;
        self.cursor_y = 1;
        self.row_height = 0;

        for (key, mut entry) in kept {
            let (Some(x), Some(y)) = self.allocate(entry.width, entry.height) else {
                continue;
            };
            self.cursor_x = x + entry.width + 1;
            self.row_height = self.row_height.max(entry.height);
            upload(
                &self.texture,
                queue,
                x,
                y,
                entry.width,
                entry.height,
                &entry.coverage,
            );
            entry.uv = uv_for(x, y, entry.width, entry.height);
            self.entries.insert(key, entry);
        }
    }
}

fn uv_for(x: u32, y: u32, width: u32, height: u32) -> [f32; 4] {
    let size = ATLAS_SIZE as f32;
    [
        x as f32 / size,
        y as f32 / size,
        (x + width) as f32 / size,
        (y + height) as f32 / size,
    ]
}

fn upload(
    texture: &wgpu::Texture,
    queue: &wgpu::Queue,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    coverage: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        coverage,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn create_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gloss glyph atlas"),
        size: wgpu::Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

#[cfg(test)]
mod tests {
    #[test]
    fn eviction_keeps_the_newest_three_quarters() {
        let mut entries: Vec<(u32, u64)> = (0..8).map(|i| (i, u64::from(i))).collect();
        entries.sort_by_key(|(_, last_used)| *last_used);
        let evict = (entries.len() / 4).max(1).min(entries.len());
        entries.drain(..evict);
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].0, 2);
        assert_eq!(entries[5].0, 7);
    }

    #[test]
    fn eviction_always_drops_at_least_one() {
        let len = 3;
        assert_eq!((len / 4).max(1).min(len), 1);
        let len = 1;
        assert_eq!((len / 4).max(1).min(len), 1);
    }
}
