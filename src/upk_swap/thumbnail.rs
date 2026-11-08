//! Read-only inventory texture decoding, separate from package mutation.
use super::*;
use image::RgbaImage;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(crate) const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PIXELS_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct DecodedThumbnail {
    pub image: RgbaImage,
    pub dependencies: Vec<PathBuf>,
}

// UE3 streams contain a summary and a table of independent zlib blocks.
fn inflate(bytes: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    if expected > MAX_PACKAGE_BYTES {
        return Err("Texture stream is too large".into());
    }
    let mut r = Reader::new(bytes);
    if r.u32()? != MAGIC {
        return Err("Unknown texture compression header".into());
    }
    let block_size = r.u32()? as usize;
    let compressed_size = r.u32()? as usize;
    let decoded_size = r.u32()? as usize;
    if !(1..=1024 * 1024).contains(&block_size) || decoded_size != expected || expected == 0 {
        return Err("Invalid texture compression sizes".into());
    }
    let count = expected.div_ceil(block_size);
    if count > 1024 || count > bytes.len().saturating_sub(16) / 8 {
        return Err("Invalid texture compression block count".into());
    }
    let mut blocks = Vec::with_capacity(count);
    for index in 0..count {
        let stored = r.u32()? as usize;
        let decoded = r.u32()? as usize;
        if decoded != block_size.min(expected - index * block_size)
            || stored == 0
            || stored > bytes.len()
        {
            return Err("Invalid compressed texture block".into());
        }
        blocks.push((stored, decoded));
    }
    if compressed_size != bytes.len().saturating_sub(r.pos)
        || blocks.iter().map(|block| block.0).sum::<usize>() != compressed_size
    {
        return Err("Texture compressed byte count mismatch".into());
    }
    let mut output = Vec::with_capacity(expected);
    for (stored, decoded) in blocks {
        let mut decoder = flate2::read::ZlibDecoder::new(r.take(stored)?);
        let start = output.len();
        decoder
            .by_ref()
            .take(decoded as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|_| "Could not decompress inventory texture")?;
        if output.len() - start != decoded || decoder.total_in() != stored as u64 {
            return Err("Invalid decompressed texture size".into());
        }
    }
    Ok(output)
}

struct Payload {
    offset: usize,
    bytes: Vec<u8>,
}
struct Package {
    tables: Tables,
    payloads: Vec<Payload>,
}
struct Property {
    kind: String,
    bytes: Vec<u8>,
}

impl Package {
    fn read(bytes: &[u8], key: &[u8; 32]) -> Result<Self, String> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("Thumbnail package is too large".into());
        }
        // Inventory-only packages can be older than the mutation reader accepts.
        let (summary, _) = Summary::read_versioned(bytes, 29)?;
        if summary.name_count > 50_000
            || summary.import_count > 10_000
            || summary.export_count > 10_000
        {
            return Err("Thumbnail tables exceed the supported size".into());
        }
        let tables = read_tables(bytes, &summary, key)?;
        if tables.exports.len() > 10_000 || tables.chunks.len() > 1024 {
            return Err("Thumbnail package has too many objects".into());
        }
        let mut payloads = Vec::new();
        let mut total = 0usize;
        for chunk in &tables.chunks {
            let offset =
                usize::try_from(chunk.uncompressed_offset).map_err(|_| "Invalid asset offset")?;
            let size =
                usize::try_from(chunk.uncompressed_size).map_err(|_| "Invalid asset size")?;
            total = total
                .checked_add(size)
                .filter(|total| *total <= MAX_PACKAGE_BYTES)
                .ok_or("Thumbnail assets are too large")?;
            if size == 0 {
                continue;
            }
            let start = usize::try_from(chunk.compressed_offset)
                .map_err(|_| "Invalid compressed offset")?;
            let stored =
                usize::try_from(chunk.compressed_size).map_err(|_| "Invalid compressed size")?;
            let end = start.checked_add(stored).ok_or("Texture offset overflow")?;
            let data = bytes.get(start..end).ok_or("Truncated thumbnail chunk")?;
            let mut decrypted;
            let data = if summary.full_encryption() {
                decrypted = data.to_vec();
                crypt_ctr(
                    &mut decrypted,
                    key,
                    chunk.nonce.as_ref().ok_or("Missing asset nonce")?,
                )?;
                &decrypted
            } else {
                data
            };
            let decoded = match summary.compression {
                1 => inflate(data, size)?,
                0 if data.len() == size => data.to_vec(),
                _ => return Err("Unsupported thumbnail package compression".into()),
            };
            payloads.push(Payload {
                offset,
                bytes: decoded,
            });
        }
        if payloads.is_empty() {
            if summary.full_encryption() {
                return Err("Encrypted thumbnail has no asset chunks".into());
            }
            payloads.push(Payload {
                offset: 0,
                bytes: bytes.to_vec(),
            });
        }
        payloads.sort_by_key(|payload| payload.offset);
        for pair in payloads.windows(2) {
            if pair[0]
                .offset
                .checked_add(pair[0].bytes.len())
                .is_none_or(|end| end > pair[1].offset)
            {
                return Err("Overlapping thumbnail asset chunks".into());
            }
        }
        Ok(Self { tables, payloads })
    }
    fn name(&self, index: i32) -> Result<&str, String> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.tables.names.get(index))
            .map(|entry| entry.value.text.as_str())
            .ok_or_else(|| "Invalid thumbnail name index".into())
    }
    fn fname(&self, r: &mut Reader<'_>) -> Result<String, String> {
        let name = self.name(r.i32()?)?;
        let instance = r.i32()?;
        if instance < 0 {
            return Err("Invalid thumbnail name instance".into());
        }
        Ok(if instance == 0 {
            name.into()
        } else {
            format!("{name}_{}", instance - 1)
        })
    }
    fn class(&self, export: &ExportEntry) -> Result<&str, String> {
        if export.class_index < 0 {
            let index = export
                .class_index
                .checked_neg()
                .and_then(|n| n.checked_sub(1))
                .ok_or("Invalid thumbnail class")? as usize;
            let import = self
                .tables
                .imports
                .get(index)
                .ok_or("Missing thumbnail class")?;
            self.name(import.values[5])
        } else {
            let index = usize::try_from(export.class_index)
                .ok()
                .and_then(|n| n.checked_sub(1))
                .ok_or("Invalid thumbnail class")?;
            self.name(
                self.tables
                    .exports
                    .get(index)
                    .ok_or("Missing thumbnail class")?
                    .name[0],
            )
        }
    }
    fn object(&self, export: &ExportEntry) -> Result<Vec<u8>, String> {
        let mut position =
            usize::try_from(export.serial_offset).map_err(|_| "Invalid object offset")?;
        let size = usize::try_from(export.serial_size).map_err(|_| "Invalid object size")?;
        if size > MAX_PACKAGE_BYTES {
            return Err("Thumbnail object is too large".into());
        }
        let mut output = Vec::with_capacity(size);
        for payload in &self.payloads {
            let Some(local) = position.checked_sub(payload.offset) else {
                break;
            };
            if local >= payload.bytes.len() {
                continue;
            }
            let take = (size - output.len()).min(payload.bytes.len() - local);
            output.extend_from_slice(&payload.bytes[local..local + take]);
            position += take;
            if output.len() == size {
                return Ok(output);
            }
        }
        Err("Thumbnail object crosses missing asset data".into())
    }
    fn properties(&self, bytes: &[u8]) -> Result<(BTreeMap<String, Property>, usize), String> {
        let mut r = Reader::new(bytes);
        r.i32()?; // UObject network index precedes tagged properties.
        let mut properties = BTreeMap::new();
        for _ in 0..1024 {
            let name = self.fname(&mut r)?;
            if name == "None" {
                return Ok((properties, r.pos));
            }
            let kind = self.fname(&mut r)?;
            let size = usize::try_from(r.i32()?).map_err(|_| "Invalid texture property size")?;
            let index = r.i32()?;
            if index < 0 || size > MAX_PIXELS_BYTES {
                return Err("Invalid texture property".into());
            }
            match kind.as_str() {
                "StructProperty" | "ByteProperty" => {
                    self.fname(&mut r)?;
                }
                "BoolProperty" => {
                    r.take(1)?;
                }
                _ => {}
            }
            let value = r.take(size)?.to_vec();
            if properties
                .insert(name, Property { kind, bytes: value })
                .is_some()
            {
                return Err("Ambiguous thumbnail properties".into());
            }
        }
        Err("Too many thumbnail properties".into())
    }
    fn texture_index(&self, expected: &str) -> Result<usize, String> {
        let mut assets = Vec::new();
        let mut exact = Vec::new();
        for (index, export) in self.tables.exports.iter().enumerate() {
            if matches!(
                self.class(export)?,
                "ProductThumbnailAsset" | "ProductThumbnailAsset_TA"
            ) {
                assets.push(index);
                if self.name(export.name[0])?.eq_ignore_ascii_case(expected) {
                    exact.push(index);
                }
            }
        }
        let asset = match exact.as_slice() {
            [index] => *index,
            [] if assets.len() == 1 => assets[0],
            [] if assets.is_empty() => {
                let textures: Vec<_> = self
                    .tables
                    .exports
                    .iter()
                    .enumerate()
                    .filter(|(_, export)| {
                        self.class(export).is_ok_and(|class| class == "Texture2D")
                    })
                    .map(|(index, _)| index)
                    .collect();
                return match textures.as_slice() {
                    [index] => Ok(*index),
                    _ => Err("No unique inventory thumbnail reference".into()),
                };
            }
            _ => return Err("Several inventory thumbnails share this package".into()),
        };
        let (properties, _) = self.properties(&self.object(&self.tables.exports[asset])?)?;
        let reference = properties
            .get("Thumbnail")
            .filter(|p| p.kind == "ObjectProperty")
            .ok_or("Missing inventory thumbnail reference")?;
        let index = usize::try_from(Reader::new(&reference.bytes).i32()?)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .ok_or("External inventory thumbnail reference is unsupported")?;
        if self.class(
            self.tables
                .exports
                .get(index)
                .ok_or("Invalid thumbnail reference")?,
        )? != "Texture2D"
        {
            return Err("Inventory thumbnail does not refer to a texture".into());
        }
        Ok(index)
    }
}

struct Bulk<'a> {
    flags: u32,
    decoded: usize,
    stored: usize,
    offset: u64,
    inline: &'a [u8],
}
fn bulk<'a>(r: &mut Reader<'a>, offset_width: usize) -> Result<Bulk<'a>, String> {
    let flags = r.u32()?;
    let decoded = r.u32()? as usize;
    let stored = r.i32()?;
    let stored = if flags & 32 != 0 && decoded == 0 && stored == -1 {
        0
    } else {
        usize::try_from(stored).map_err(|_| "Invalid texture bulk size")?
    };
    if decoded > MAX_PIXELS_BYTES || stored > MAX_PIXELS_BYTES {
        return Err("Texture mip is too large".into());
    }
    let offset = if flags & 0x10000 != 0 && flags & 1 == 0 {
        0
    } else if offset_width == 8 {
        r.u64()?
    } else {
        r.u32()? as u64
    };
    let inline = if flags & (1 | 32) == 0 {
        r.take(stored)?
    } else {
        &[]
    };
    Ok(Bulk {
        flags,
        decoded,
        stored,
        offset,
        inline,
    })
}
fn pixel_bytes(
    record: &Bulk<'_>,
    directory: &Path,
    cache: Option<&str>,
) -> Result<(Vec<u8>, Option<PathBuf>), String> {
    if record.flags & 32 != 0 || record.decoded == 0 {
        return Err("Texture mip has no image data".into());
    }
    let dependency = if record.flags & 1 != 0 {
        let cache = cache
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            })
            .ok_or("Missing or invalid texture cache reference")?;
        Some(directory.join(format!("{cache}.tfc")))
    } else {
        None
    };
    let stored = if let Some(path) = &dependency {
        let mut file =
            std::fs::File::open(path).map_err(|_| "Inventory texture cache is unavailable")?;
        let len = file
            .metadata()
            .map_err(|_| "Could not inspect texture cache")?
            .len();
        if record
            .offset
            .checked_add(record.stored as u64)
            .is_none_or(|end| end > len)
        {
            return Err("Inventory texture cache range is invalid".into());
        }
        file.seek(SeekFrom::Start(record.offset))
            .map_err(|_| "Could not seek inventory texture cache")?;
        let mut bytes = vec![0; record.stored];
        file.read_exact(&mut bytes)
            .map_err(|_| "Could not read inventory texture cache")?;
        bytes
    } else {
        record.inline.to_vec()
    };
    let decoded = if record.flags & 2 != 0 {
        inflate(&stored, record.decoded)?
    } else if record.flags & (16 | 128) != 0 {
        return Err("Unsupported inventory mip compression".into());
    } else {
        stored
    };
    if decoded.len() != record.decoded {
        return Err("Inventory texture byte count mismatch".into());
    }
    Ok((decoded, dependency))
}
fn pixels(bytes: &[u8], width: u32, height: u32, format: &str) -> Result<RgbaImage, String> {
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err("Invalid inventory texture dimensions".into());
    }
    let (w, h) = (width as usize, height as usize);
    let block_size = match format {
        "PF_DXT1" => 8,
        "PF_DXT3" | "PF_DXT5" | "PF_BC7" => 16,
        "PF_A8R8G8B8" | "PF_G8" => 0,
        _ => return Err("Unsupported inventory texture format".into()),
    };
    let expected = match format {
        "PF_G8" => w * h,
        "PF_A8R8G8B8" => w * h * 4,
        _ => w.div_ceil(4) * h.div_ceil(4) * block_size,
    };
    if bytes.len() != expected {
        return Err("Invalid inventory pixel byte count".into());
    }
    let mut image = RgbaImage::new(width, height);
    if format == "PF_G8" {
        for (pixel, value) in image.pixels_mut().zip(bytes) {
            *pixel = image::Rgba([*value, *value, *value, 255]);
        }
    } else if format == "PF_A8R8G8B8" {
        for (pixel, bgra) in image.pixels_mut().zip(bytes.as_chunks::<4>().0) {
            *pixel = image::Rgba([bgra[2], bgra[1], bgra[0], bgra[3]]);
        }
    } else {
        for y in 0..h.div_ceil(4) {
            for x in 0..w.div_ceil(4) {
                let offset = (y * w.div_ceil(4) + x) * block_size;
                let data = &bytes[offset..offset + block_size];
                let mut rgba = [0; 64];
                match format {
                    "PF_DXT1" => bcdec_rs::bc1(data, &mut rgba, 16),
                    "PF_DXT3" => bcdec_rs::bc2(data, &mut rgba, 16),
                    "PF_DXT5" => bcdec_rs::bc3(data, &mut rgba, 16),
                    "PF_BC7" => bcdec_rs::bc7(data, &mut rgba, 16),
                    _ => unreachable!(),
                }
                for by in 0..4 {
                    for bx in 0..4 {
                        if x * 4 + bx < w && y * 4 + by < h {
                            let i = (by * 4 + bx) * 4;
                            image.put_pixel(
                                (x * 4 + bx) as u32,
                                (y * 4 + by) as u32,
                                image::Rgba(rgba[i..i + 4].try_into().unwrap()),
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(image)
}
fn texture(
    package: &Package,
    bytes: &[u8],
    directory: &Path,
    offset_width: usize,
) -> Result<DecodedThumbnail, String> {
    let (properties, offset) = package.properties(bytes)?;
    let format = properties
        .get("Format")
        .ok_or("Inventory texture format is missing")?;
    let format = package.fname(&mut Reader::new(&format.bytes))?;
    let cache = properties
        .get("TextureFileCacheName")
        .map(|p| package.fname(&mut Reader::new(&p.bytes)))
        .transpose()?;
    let mut r = Reader::new(&bytes[offset..]);
    bulk(&mut r, offset_width)?; // Source artwork; use cooked mips instead.
    let count = r.u32()?;
    if !(1..=16).contains(&count) {
        return Err("Invalid inventory mip count".into());
    }
    let mut mips = Vec::new();
    for _ in 0..count {
        let record = bulk(&mut r, offset_width)?;
        let width = r.u32()?;
        let height = r.u32()?;
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err("Invalid inventory mip dimensions".into());
        }
        if record.decoded > 0 && record.flags & 32 == 0 {
            mips.push((width, height, record));
        }
    }
    mips.sort_by_key(|(w, h, _)| ((*w).max(*h).abs_diff(256), std::cmp::Reverse(*w)));
    for (width, height, record) in mips {
        if let Ok((bytes, dependency)) = pixel_bytes(&record, directory, cache.as_deref())
            && let Ok(image) = pixels(&bytes, width, height, &format)
        {
            let image = if width > 256 || height > 256 {
                image::DynamicImage::ImageRgba8(image)
                    .thumbnail(256, 256)
                    .to_rgba8()
            } else {
                image
            };
            return Ok(DecodedThumbnail {
                image,
                dependencies: dependency.into_iter().collect(),
            });
        }
    }
    Err("No supported inventory image data is available".into())
}

pub(crate) fn decode(
    bytes: &[u8],
    key: &[u8; 32],
    directory: &Path,
    expected_asset: &str,
) -> Result<DecodedThumbnail, String> {
    let package = Package::read(bytes, key)?;
    let index = package.texture_index(expected_asset)?;
    let bytes = package.object(&package.tables.exports[index])?;
    // Validate the full mip table when resolving legacy bulk-offset widths.
    match (
        texture(&package, &bytes, directory, 8),
        texture(&package, &bytes, directory, 4),
    ) {
        (Ok(a), Ok(b)) if a.image != b.image => Err("Ambiguous inventory texture layout".into()),
        (Ok(image), _) | (_, Ok(image)) => Ok(image),
        (Err(error), Err(_)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_preview_support_does_not_expand_mutation_support() {
        let key = [7; 32];
        let bytes = super::super::tests::fixture_with_names("Inventory", &key, 1, 29, &[]);
        assert!(Package::read(&bytes, &key).is_ok());
        assert!(Summary::read(&bytes).is_err());
        assert!(Package::read(&bytes, &[9; 32]).is_err());
    }

    #[test]
    fn thumbnail_reference_selects_named_asset_and_rejects_ambiguity() {
        let names = [
            "Texture2D",
            "ProductThumbnailAsset_TA",
            "Other_T",
            "Wanted_T",
            "OtherImage",
            "WantedImage",
            "None",
            "Thumbnail",
            "ObjectProperty",
        ];
        let export = |name, class_index, serial_offset, serial_size| ExportEntry {
            class_index,
            super_index: 0,
            outer_index: 0,
            name: [name, 0],
            archetype: 0,
            flags: 0,
            serial_size,
            serial_offset,
            export_flags: 0,
            net: vec![],
            guid: [0; 16],
            package_flags: 0,
        };
        let mut bytes = Vec::new();
        for reference in [3i32, 4] {
            for word in [-1, 7, 0, 8, 0, 4, 0, reference, 6, 0] {
                bytes.extend(word.to_le_bytes());
            }
        }
        let package = Package {
            tables: Tables {
                names: names
                    .iter()
                    .map(|name| NameEntry {
                        value: FString {
                            text: (*name).into(),
                            unicode: false,
                        },
                        flags: 0,
                    })
                    .collect(),
                imports: vec![
                    ImportEntry {
                        values: [0, 0, 0, 0, 0, 0, 0],
                    },
                    ImportEntry {
                        values: [0, 0, 0, 0, 0, 1, 0],
                    },
                ],
                exports: vec![
                    export(2, -2, 0, 40),
                    export(3, -2, 40, 40),
                    export(4, -1, 80, 0),
                    export(5, -1, 80, 0),
                ],
                chunks: vec![],
            },
            payloads: vec![Payload { offset: 0, bytes }],
        };
        assert_eq!(package.texture_index("Wanted_T").unwrap(), 3);
        assert_eq!(package.texture_index("Other_T").unwrap(), 2);
        assert!(package.texture_index("Unknown_T").is_err());
    }
    #[test]
    fn pixels_check_lengths_bgra_channels_and_bc1_transparency() {
        assert_eq!(
            pixels(&[1, 2, 3, 4], 1, 1, "PF_A8R8G8B8").unwrap().as_raw(),
            &[3, 2, 1, 4]
        );
        assert!(pixels(&[1, 2, 3], 1, 1, "PF_A8R8G8B8").is_err());
        assert!(pixels(&[], 0, 0, "PF_DXT1").is_err());
        let transparent = pixels(&[0, 0, 255, 255, 255, 255, 255, 255], 3, 2, "PF_DXT1").unwrap();
        assert!(transparent.pixels().all(|pixel| pixel[3] == 0));
    }
    #[test]
    fn compressed_stream_checks_declared_and_actual_lengths() {
        use std::io::Write;
        let data = b"inventory image fixture";
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        let stored = encoder.finish().unwrap();
        let mut bytes = Vec::new();
        for word in [
            MAGIC,
            131072,
            stored.len() as u32,
            data.len() as u32,
            stored.len() as u32,
            data.len() as u32,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(stored);
        assert_eq!(inflate(&bytes, data.len()).unwrap(), data);
        assert!(inflate(&bytes, data.len() + 1).is_err());
        assert!(inflate(&bytes[..bytes.len() - 1], data.len()).is_err());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        assert!(inflate(&bytes, data.len()).is_err());
    }
    #[test]
    #[ignore = "requires RL_INSTALL, RL_KEY_INDEX and RL_THUMBNAIL_OUTPUT; reads game files only"]
    fn installed_thumbnails_decode_without_modifying_game_files() {
        let root =
            PathBuf::from(std::env::var("RL_INSTALL").unwrap()).join("TAGame/CookedPCConsole");
        let csv = std::fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let output = PathBuf::from(std::env::var("RL_THUMBNAIL_OUTPUT").unwrap());
        std::fs::create_dir_all(&output).unwrap();
        for base in [
            "Boost_AlphaReward",
            "Boost_Standard",
            "Body_Octane",
            "Antenna_8Ball",
            "WHEEL_AlphaRim",
            "SS_ScifiStreak",
            "Explosion_Default",
            "Hat_AlphaHat",
            "PaintFinish_BrushedMetal",
        ] {
            let key = csv
                .lines()
                .skip(1)
                .find_map(|line| {
                    let fields: Vec<_> = line.split(',').collect();
                    (fields.len() >= 9 && fields[7].trim_matches('"').eq_ignore_ascii_case(base))
                        .then(|| key_from_base64(fields[8].trim_matches('"')).unwrap())
                })
                .unwrap();
            let path = root.join(format!("{base}_T_SF.upk"));
            let bytes = std::fs::read(&path).unwrap();
            let before = std::fs::metadata(&path).unwrap().modified().unwrap();
            let decoded = decode(&bytes, &key, &root, &format!("{base}_T"))
                .unwrap_or_else(|error| panic!("{base}: {error}"));
            assert!(decoded.image.width() <= 256 && decoded.image.height() <= 256);
            decoded
                .image
                .save(output.join(format!("{base}.png")))
                .unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(
                std::fs::metadata(&path).unwrap().modified().unwrap(),
                before
            );
            println!(
                "{base}: {}x{}",
                decoded.image.width(),
                decoded.image.height()
            );
        }
    }
}
