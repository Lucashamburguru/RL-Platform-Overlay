//! Optional software rendering of egui's actual meshes for offline visual review.
//! Test-only: no application startup, game access or external services required.
use eframe::egui;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct ReviewRenderer {
    textures: HashMap<egui::TextureId, egui::ColorImage>,
}

impl ReviewRenderer {
    pub fn capture(
        &mut self,
        ctx: &egui::Context,
        output: &egui::FullOutput,
        size: [f32; 2],
        path: Option<&std::path::Path>,
    ) {
        for (id, delta) in &output.textures_delta.set {
            let image = match &delta.image {
                egui::ImageData::Color(image) => (**image).clone(),
                egui::ImageData::Font(image) => egui::ColorImage {
                    size: image.size,
                    pixels: image.srgba_pixels(None).collect(),
                },
            };
            if let Some([x, y]) = delta.pos {
                let target = self
                    .textures
                    .get_mut(id)
                    .expect("partial texture requires original");
                for row in 0..image.height() {
                    let start = (y + row) * target.width() + x;
                    target.pixels[start..start + image.width()].copy_from_slice(
                        &image.pixels[row * image.width()..(row + 1) * image.width()],
                    );
                }
            } else {
                self.textures.insert(*id, image);
            }
        }
        if let Some(path) = path {
            let scale = output.pixels_per_point;
            let mut canvas = image::RgbaImage::from_pixel(
                (size[0] * scale) as u32,
                (size[1] * scale) as u32,
                image::Rgba([27, 27, 27, 255]),
            );
            for clipped in ctx.tessellate(output.shapes.clone(), scale) {
                let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
                    continue;
                };
                let Some(texture) = self.textures.get(&mesh.texture_id) else {
                    continue;
                };
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let vertices = triangle.map(|index| mesh.vertices[index as usize]);
                    let points = vertices.map(|vertex| vertex.pos * scale);
                    let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
                        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
                    };
                    let area = edge(points[0], points[1], points[2]);
                    if area.abs() < 0.0001 {
                        continue;
                    }
                    let min_x = points
                        .iter()
                        .map(|p| p.x)
                        .fold(f32::INFINITY, f32::min)
                        .max(clipped.clip_rect.left() * scale)
                        .max(0.0)
                        .floor() as u32;
                    let min_y = points
                        .iter()
                        .map(|p| p.y)
                        .fold(f32::INFINITY, f32::min)
                        .max(clipped.clip_rect.top() * scale)
                        .max(0.0)
                        .floor() as u32;
                    let max_x = points
                        .iter()
                        .map(|p| p.x)
                        .fold(0.0, f32::max)
                        .min(clipped.clip_rect.right() * scale)
                        .min(canvas.width() as f32)
                        .ceil() as u32;
                    let max_y = points
                        .iter()
                        .map(|p| p.y)
                        .fold(0.0, f32::max)
                        .min(clipped.clip_rect.bottom() * scale)
                        .min(canvas.height() as f32)
                        .ceil() as u32;
                    for y in min_y..max_y {
                        for x in min_x..max_x {
                            let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                            let weights = [
                                edge(points[1], points[2], p) / area,
                                edge(points[2], points[0], p) / area,
                                edge(points[0], points[1], p) / area,
                            ];
                            if weights.iter().any(|w| *w < 0.0) {
                                continue;
                            }
                            let u: f32 = (0..3).map(|i| weights[i] * vertices[i].uv.x).sum();
                            let v: f32 = (0..3).map(|i| weights[i] * vertices[i].uv.y).sum();
                            let tx =
                                ((u * texture.width() as f32) as usize).min(texture.width() - 1);
                            let ty =
                                ((v * texture.height() as f32) as usize).min(texture.height() - 1);
                            let texel = texture.pixels[ty * texture.width() + tx].to_array();
                            let color: [f32; 4] = std::array::from_fn(|channel| {
                                (0..3)
                                    .map(|i| weights[i] * f32::from(vertices[i].color[channel]))
                                    .sum::<f32>()
                                    * f32::from(texel[channel])
                                    / 255.0
                            });
                            let target = canvas.get_pixel_mut(x, y);
                            for channel in 0..3 {
                                target[channel] = (color[channel]
                                    + f32::from(target[channel]) * (1.0 - color[3] / 255.0))
                                    .min(255.0)
                                    as u8;
                            }
                        }
                    }
                }
            }
            canvas.save(path).expect("save review screenshot");
        }
        for id in &output.textures_delta.free {
            self.textures.remove(id);
        }
    }
}
