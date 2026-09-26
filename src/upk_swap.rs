//! Rocket League UPK package identity rewriting.

use aes::Aes256;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use base64::Engine as _;

const MAGIC: u32 = 0x9e2a83c1;
const FULL_ENCRYPTION: u32 = 0x0800;

#[derive(Clone, Debug)]
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(len).ok_or("UPK offset overflow")?;
        let value = self.bytes.get(self.pos..end).ok_or("Truncated UPK data")?;
        self.pos = end;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn array<T>(
        &mut self,
        mut read: impl FnMut(&mut Self) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        let count = self.i32()?;
        if !(0..=1_000_000).contains(&count) {
            return Err("Invalid UPK array length".into());
        }
        (0..count).map(|_| read(self)).collect()
    }
}

#[derive(Clone, Debug)]
struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    fn pos(&self) -> usize {
        self.bytes.len()
    }
    fn raw(&mut self, v: &[u8]) {
        self.bytes.extend_from_slice(v)
    }
    fn u16(&mut self, v: u16) {
        self.raw(&v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) {
        self.raw(&v.to_le_bytes())
    }
    fn i32(&mut self, v: i32) {
        self.raw(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) {
        self.raw(&v.to_le_bytes())
    }
    fn i64(&mut self, v: i64) {
        self.raw(&v.to_le_bytes())
    }
    fn array<T>(&mut self, values: &[T], mut write: impl FnMut(&mut Self, &T)) {
        self.i32(values.len() as i32);
        for v in values {
            write(self, v)
        }
    }
}

#[derive(Clone, Debug)]
struct FString {
    text: String,
    unicode: bool,
}
impl FString {
    fn read(r: &mut Reader<'_>) -> Result<Self, String> {
        let len = r.i32()?;
        if len == 0 || len.unsigned_abs() > 1_000_000 {
            return Err("Invalid UPK string length".into());
        }
        if len < 0 {
            let count = len.unsigned_abs() as usize;
            let raw = r.take(count.checked_mul(2).ok_or("UPK string overflow")?)?;
            if raw.len() < 2 || raw[raw.len() - 2..] != [0, 0] {
                return Err("Invalid UTF-16 UPK terminator".into());
            }
            let words = raw[..raw.len() - 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect::<Vec<_>>();
            Ok(Self {
                text: String::from_utf16(&words).map_err(|_| "Invalid UTF-16 UPK string")?,
                unicode: true,
            })
        } else {
            let raw = r.take(len as usize)?;
            if raw.last() != Some(&0) {
                return Err("Invalid UPK string terminator".into());
            }
            Ok(Self {
                text: String::from_utf8(raw[..raw.len() - 1].to_vec())
                    .map_err(|_| "Invalid UTF-8 UPK string")?,
                unicode: false,
            })
        }
    }
    fn write(&self, w: &mut Writer) {
        if self.unicode {
            let words = self.text.encode_utf16().collect::<Vec<_>>();
            w.i32(-((words.len() + 1) as i32));
            for v in words {
                w.u16(v)
            }
            w.u16(0)
        } else {
            w.i32((self.text.len() + 1) as i32);
            w.raw(self.text.as_bytes());
            w.raw(&[0])
        }
    }
}

#[derive(Clone, Debug)]
struct Generation {
    exports: i32,
    names: i32,
    unknown: i32,
}
#[derive(Clone, Debug)]
struct SummaryUnknown {
    values: [i32; 5],
    list: Vec<i32>,
}
#[derive(Clone, Debug)]
struct Chunk {
    uncompressed_offset: i64,
    uncompressed_size: i32,
    compressed_offset: i64,
    compressed_size: i32,
    nonce: Option<[u8; 12]>,
}
impl Chunk {
    fn read(r: &mut Reader<'_>, modern: bool) -> Result<Self, String> {
        Ok(Self {
            uncompressed_offset: r.i64()?,
            uncompressed_size: r.i32()?,
            compressed_offset: r.i64()?,
            compressed_size: r.i32()?,
            nonce: if modern {
                Some(r.take(12)?.try_into().unwrap())
            } else {
                None
            },
        })
    }
    fn write(&self, w: &mut Writer) {
        w.i64(self.uncompressed_offset);
        w.i32(self.uncompressed_size);
        w.i64(self.compressed_offset);
        w.i32(self.compressed_size);
        if let Some(v) = self.nonce {
            w.raw(&v)
        }
    }
}

#[derive(Clone, Debug)]
struct Summary {
    tag: u32,
    version: u16,
    licensee: u16,
    total_header: i32,
    folder: FString,
    package_flags: u32,
    name_count: i32,
    name_offset: i32,
    export_count: i32,
    export_offset: i32,
    import_count: i32,
    import_offset: i32,
    depends_offset: i32,
    unknown: [i32; 4],
    guid: [u8; 16],
    generations: Vec<Generation>,
    engine: u32,
    cooker: u32,
    compression: i32,
    summary_chunks: Vec<Chunk>,
    unknown5: i32,
    strings: Vec<FString>,
    unknown_records: Vec<SummaryUnknown>,
    garbage_size: i32,
    chunk_info_offset: i32,
    last_aes_block_size: i32,
    header_nonce: Option<[u8; 12]>,
}
impl Summary {
    fn modern(&self) -> bool {
        self.licensee >= 33
    }
    fn full_encryption(&self) -> bool {
        self.package_flags & FULL_ENCRYPTION != 0
    }
    fn encrypted_len(&self) -> Result<usize, String> {
        let n = self
            .total_header
            .checked_sub(self.garbage_size)
            .and_then(|n| n.checked_sub(self.name_offset))
            .ok_or("Invalid encrypted header size")?;
        if n < 0 {
            return Err("Invalid encrypted header size".into());
        }
        if self.full_encryption() {
            Ok(n as usize)
        } else {
            Ok(((n as usize) + 15) & !15)
        }
    }
    fn read(bytes: &[u8]) -> Result<(Self, usize), String> {
        let mut r = Reader::new(bytes);
        let tag = r.u32()?;
        let version = r.u16()?;
        let licensee = r.u16()?;
        if tag != MAGIC {
            return Err("Invalid UPK magic".into());
        }
        if version != 868 || licensee < 32 {
            return Err(format!("Unsupported UPK version {version}/{licensee}"));
        }
        let total_header = r.i32()?;
        let folder = FString::read(&mut r)?;
        let package_flags = r.u32()?;
        let name_count = r.i32()?;
        let name_offset = r.i32()?;
        let export_count = r.i32()?;
        let export_offset = r.i32()?;
        let import_count = r.i32()?;
        let import_offset = r.i32()?;
        let depends_offset = r.i32()?;
        let unknown = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
        let guid = r.take(16)?.try_into().unwrap();
        let generations = r.array(|r| {
            Ok(Generation {
                exports: r.i32()?,
                names: r.i32()?,
                unknown: r.i32()?,
            })
        })?;
        let engine = r.u32()?;
        let cooker = r.u32()?;
        let compression = r.i32()?;
        let modern = licensee >= 33;
        let summary_chunks = r.array(|r| Chunk::read(r, modern))?;
        let unknown5 = r.i32()?;
        let strings = r.array(FString::read)?;
        let unknown_records = r.array(|r| {
            Ok(SummaryUnknown {
                values: [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?],
                list: r.array(|r| r.i32())?,
            })
        })?;
        let garbage_size = r.i32()?;
        let chunk_info_offset = r.i32()?;
        let last_aes_block_size = r.i32()?;
        let header_nonce = if modern {
            Some(r.take(12)?.try_into().unwrap())
        } else {
            None
        };
        if package_flags & FULL_ENCRYPTION != 0 && header_nonce.is_none() {
            return Err("Fully encrypted UPK requires a header nonce".into());
        }
        if name_count < 0
            || export_count < 0
            || import_count < 0
            || name_offset < r.pos as i32
            || total_header < name_offset
            || total_header as usize > bytes.len()
            || garbage_size < 0
        {
            return Err("Invalid UPK summary offsets".into());
        }
        Ok((
            Self {
                tag,
                version,
                licensee,
                total_header,
                folder,
                package_flags,
                name_count,
                name_offset,
                export_count,
                export_offset,
                import_count,
                import_offset,
                depends_offset,
                unknown,
                guid,
                generations,
                engine,
                cooker,
                compression,
                summary_chunks,
                unknown5,
                strings,
                unknown_records,
                garbage_size,
                chunk_info_offset,
                last_aes_block_size,
                header_nonce,
            },
            r.pos,
        ))
    }
    fn write(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(self.tag);
        w.u16(self.version);
        w.u16(self.licensee);
        w.i32(self.total_header);
        self.folder.write(&mut w);
        w.u32(self.package_flags);
        w.i32(self.name_count);
        w.i32(self.name_offset);
        w.i32(self.export_count);
        w.i32(self.export_offset);
        w.i32(self.import_count);
        w.i32(self.import_offset);
        w.i32(self.depends_offset);
        for v in self.unknown {
            w.i32(v)
        }
        w.raw(&self.guid);
        w.array(&self.generations, |w, g| {
            w.i32(g.exports);
            w.i32(g.names);
            w.i32(g.unknown)
        });
        w.u32(self.engine);
        w.u32(self.cooker);
        w.i32(self.compression);
        w.array(&self.summary_chunks, |w, c| c.write(w));
        w.i32(self.unknown5);
        w.array(&self.strings, |w, s| s.write(w));
        w.array(&self.unknown_records, |w, u| {
            for v in u.values {
                w.i32(v)
            }
            w.array(&u.list, |w, v| w.i32(*v))
        });
        w.i32(self.garbage_size);
        w.i32(self.chunk_info_offset);
        w.i32(self.last_aes_block_size);
        if let Some(nonce) = self.header_nonce {
            w.raw(&nonce)
        }
        w.bytes
    }
}

#[derive(Clone, Debug)]
struct NameEntry {
    value: FString,
    flags: u64,
}
#[derive(Clone, Debug)]
struct ImportEntry {
    values: [i32; 7],
}
#[derive(Clone, Debug)]
struct ExportEntry {
    class_index: i32,
    super_index: i32,
    outer_index: i32,
    name: [i32; 2],
    archetype: i32,
    flags: u64,
    serial_size: i32,
    serial_offset: i64,
    export_flags: i32,
    net: Vec<i32>,
    guid: [u8; 16],
    package_flags: i32,
}
impl ExportEntry {
    fn read(r: &mut Reader<'_>) -> Result<Self, String> {
        Ok(Self {
            class_index: r.i32()?,
            super_index: r.i32()?,
            outer_index: r.i32()?,
            name: [r.i32()?, r.i32()?],
            archetype: r.i32()?,
            flags: r.u64()?,
            serial_size: r.i32()?,
            serial_offset: r.i64()?,
            export_flags: r.i32()?,
            net: r.array(|r| r.i32())?,
            guid: r.take(16)?.try_into().unwrap(),
            package_flags: r.i32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.i32(self.class_index);
        w.i32(self.super_index);
        w.i32(self.outer_index);
        w.i32(self.name[0]);
        w.i32(self.name[1]);
        w.i32(self.archetype);
        w.u64(self.flags);
        w.i32(self.serial_size);
        w.i64(self.serial_offset);
        w.i32(self.export_flags);
        w.array(&self.net, |w, v| w.i32(*v));
        w.raw(&self.guid);
        w.i32(self.package_flags)
    }
}
#[derive(Clone, Debug)]
struct Tables {
    names: Vec<NameEntry>,
    imports: Vec<ImportEntry>,
    exports: Vec<ExportEntry>,
    chunks: Vec<Chunk>,
}

fn crypt(bytes: &mut [u8], key: &[u8; 32], decrypt: bool) {
    let cipher = Aes256::new(key.into());
    for raw in bytes.as_chunks_mut::<16>().0 {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(raw);
        if decrypt {
            cipher.decrypt_block(block)
        } else {
            cipher.encrypt_block(block)
        }
    }
}

// Fully encrypted packages use a 96-bit nonce followed by a big-endian
// 32-bit counter starting at zero, independently for the header and each chunk.
fn crypt_ctr(bytes: &mut [u8], key: &[u8; 32], nonce: &[u8; 12]) -> Result<(), String> {
    let cipher = Aes256::new(key.into());
    for (index, chunk) in bytes.chunks_mut(16).enumerate() {
        let counter = u32::try_from(index).map_err(|_| "UPK AES counter overflow")?;
        let mut block = aes::cipher::Block::<Aes256>::default();
        block[..12].copy_from_slice(nonce);
        block[12..].copy_from_slice(&counter.to_be_bytes());
        cipher.encrypt_block(&mut block);
        for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= mask;
        }
    }
    Ok(())
}

fn crypt_header(
    bytes: &mut [u8],
    s: &Summary,
    key: &[u8; 32],
    decrypt: bool,
) -> Result<(), String> {
    if s.full_encryption() {
        crypt_ctr(
            bytes,
            key,
            s.header_nonce.as_ref().ok_or("Missing UPK header nonce")?,
        )
    } else {
        crypt(bytes, key, decrypt);
        Ok(())
    }
}
fn read_tables(bytes: &[u8], s: &Summary, key: &[u8; 32]) -> Result<Tables, String> {
    let len = s.encrypted_len()?;
    let start = s.name_offset as usize;
    let mut plain = bytes
        .get(start..start + len)
        .ok_or("Truncated encrypted UPK header")?
        .to_vec();
    crypt_header(&mut plain, s, key, true)?;
    let mut r = Reader::new(&plain);
    let names = (0..s.name_count)
        .map(|_| {
            Ok(NameEntry {
                value: FString::read(&mut r)?,
                flags: r.u64()?,
            })
        })
        .collect::<Result<_, String>>()?;
    let imports = (0..s.import_count)
        .map(|_| {
            let mut values = [0; 7];
            for v in &mut values {
                *v = r.i32()?
            }
            Ok(ImportEntry { values })
        })
        .collect::<Result<_, String>>()?;
    let exports = (0..s.export_count)
        .map(|_| ExportEntry::read(&mut r))
        .collect::<Result<_, _>>()?;
    let chunks = r.array(|r| Chunk::read(r, s.modern()))?;
    Ok(Tables {
        names,
        imports,
        exports,
        chunks,
    })
}
fn has_identity(t: &Tables, id: &str) -> bool {
    let sf = format!("{id}_SF");
    t.names.iter().any(|n| n.value.text == id) && t.names.iter().any(|n| n.value.text == sf)
}

pub fn key_from_base64(v: &str) -> Result<[u8; 32], String> {
    base64::engine::general_purpose::STANDARD
        .decode(v)
        .map_err(|e| format!("Invalid UPK key: {e}"))?
        .try_into()
        .map_err(|_| "UPK AES key must be 32 bytes".into())
}
pub fn package_guid(data: &[u8]) -> Result<[u8; 16], String> {
    Ok(Summary::read(data)?.0.guid)
}
#[allow(dead_code)]
pub fn package_version(data: &[u8]) -> Result<(u16, u16), String> {
    let s = Summary::read(data)?.0;
    Ok((s.version, s.licensee))
}
pub fn validate_identity(data: &[u8], key: &[u8; 32], id: &str) -> Result<(), String> {
    let (s, _) = Summary::read(data)?;
    let t = read_tables(data, &s, key)?;
    if has_identity(&t, id) {
        Ok(())
    } else {
        Err(format!("Package identity does not match {id}"))
    }
}

#[cfg(not(feature = "microsoft-store"))]
pub fn boost_sound_banks(data: &[u8], key: &[u8; 32]) -> Result<Vec<String>, String> {
    let (summary, _) = Summary::read(data)?;
    let tables = read_tables(data, &summary, key)?;
    let mut banks = tables
        .names
        .into_iter()
        .map(|n| n.value.text)
        .filter(|name| name.starts_with("SFX_Boost"))
        .map(|name| format!("{name}.bnk"))
        .collect::<Vec<_>>();
    banks.sort();
    banks.dedup();
    Ok(banks)
}

pub fn masquerade(
    donor: &[u8],
    target: &[u8],
    donor_key: &[u8; 32],
    target_key: &[u8; 32],
    donor_id: &str,
    target_id: &str,
) -> Result<Vec<u8>, String> {
    let (mut ds, summary_len) = Summary::read(donor)?;
    let donor_header_end = ds.total_header as usize;
    let (ts, _) = Summary::read(target)?;
    let mut tables = read_tables(donor, &ds, donor_key)?;
    let target_tables = read_tables(target, &ts, target_key)?;
    if !has_identity(&tables, donor_id) {
        return Err("Donor key or identity does not match installed UPK".into());
    }
    if !has_identity(&target_tables, target_id) {
        return Err("Target key or identity does not match installed UPK".into());
    }
    let donor_sf = format!("{donor_id}_SF");
    let target_sf = format!("{target_id}_SF");
    for n in &mut tables.names {
        if n.value.text == donor_id {
            n.value.text = target_id.into()
        } else if n.value.text == donor_sf {
            n.value.text = target_sf.clone()
        }
    }
    ds.guid = ts.guid;
    let old_encrypted = ds.encrypted_len()? as i64;
    let padding = (ds.name_offset as usize)
        .checked_sub(summary_len)
        .ok_or("Invalid summary padding")?;
    let mut header = Writer::new();
    ds.name_offset = (summary_len + padding) as i32;
    for n in &tables.names {
        n.value.write(&mut header);
        header.u64(n.flags)
    }
    ds.import_offset = (summary_len + padding + header.pos()) as i32;
    for i in &tables.imports {
        for v in i.values {
            header.i32(v)
        }
    }
    ds.export_offset = (summary_len + padding + header.pos()) as i32;
    let export_local = header.pos();
    for e in &tables.exports {
        e.write(&mut header)
    }
    ds.depends_offset = (summary_len + padding + header.pos()) as i32;
    ds.chunk_info_offset = header.pos() as i32;
    let chunk_local = header.pos();
    header.array(&tables.chunks, |w, c| c.write(w));
    let unpadded = header.pos();
    let padded = if ds.full_encryption() {
        unpadded
    } else {
        (unpadded + 15) & !15
    };
    for pos in unpadded..padded {
        header.raw(&[(pos % 0xff) as u8])
    }
    let delta = padded as i64 - old_encrypted;
    ds.total_header = (ds.name_offset as usize + unpadded + ds.garbage_size as usize) as i32;
    for e in &mut tables.exports {
        e.serial_offset = e
            .serial_offset
            .checked_add(delta)
            .ok_or("Export offset overflow")?
    }
    let mut rewritten = Writer::new();
    for e in &tables.exports {
        e.write(&mut rewritten)
    }
    header.bytes[export_local..export_local + rewritten.bytes.len()]
        .copy_from_slice(&rewritten.bytes);
    for c in &mut tables.chunks {
        c.compressed_offset = c
            .compressed_offset
            .checked_add(delta)
            .ok_or("Chunk offset overflow")?;
        if c.uncompressed_size != 0 {
            c.uncompressed_offset = c
                .uncompressed_offset
                .checked_add(delta)
                .ok_or("Chunk offset overflow")?
        }
    }
    let mut rewritten = Writer::new();
    rewritten.array(&tables.chunks, |w, c| c.write(w));
    header.bytes[chunk_local..chunk_local + rewritten.bytes.len()]
        .copy_from_slice(&rewritten.bytes);
    let mut payload_start = ds.name_offset as usize + old_encrypted as usize;
    if ds.full_encryption() {
        // The verification bytes following the tables are part of the same CTR
        // stream. Decrypt them at their old position and encrypt them again at
        // their new position after resizing the names.
        let mut old_header = donor[ds.name_offset as usize..donor_header_end].to_vec();
        crypt_header(&mut old_header, &ds, donor_key, true)?;
        header.bytes.extend_from_slice(
            old_header
                .get(old_encrypted as usize..)
                .ok_or("Invalid UPK verification data")?,
        );
        payload_start = donor_header_end;
    }
    crypt_header(&mut header.bytes, &ds, target_key, false)?;
    let mut out = ds.write();
    if out.len() != summary_len {
        return Err("UPK summary size changed unexpectedly".into());
    }
    out.resize(out.len() + padding, 0);
    out.extend_from_slice(&header.bytes);
    out.extend_from_slice(
        donor
            .get(payload_start..)
            .ok_or("Truncated donor payload")?,
    );
    if ds.full_encryption() {
        // Asset chunks have their own nonces and remain encrypted under the
        // donor key until explicitly rekeyed for the target package.
        let mut previous_end = ds.total_header as usize;
        for chunk in &tables.chunks {
            let start = usize::try_from(chunk.compressed_offset)
                .map_err(|_| "Invalid encrypted chunk offset")?;
            let size = usize::try_from(chunk.compressed_size)
                .map_err(|_| "Invalid encrypted chunk size")?;
            let end = start.checked_add(size).ok_or("Encrypted chunk overflow")?;
            if start < previous_end {
                return Err("Overlapping encrypted UPK chunks".into());
            }
            let bytes = out
                .get_mut(start..end)
                .ok_or("Truncated encrypted UPK chunk")?;
            let nonce = chunk.nonce.as_ref().ok_or("Missing UPK chunk nonce")?;
            crypt_ctr(bytes, donor_key, nonce)?;
            crypt_ctr(bytes, target_key, nonce)?;
            previous_end = end;
        }
    }
    validate_identity(&out, target_key, target_id)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(id: &str, key: &[u8; 32], guid: u8, licensee: u16) -> Vec<u8> {
        let mut s = Summary {
            tag: MAGIC,
            version: 868,
            licensee,
            total_header: 0,
            folder: FString {
                text: String::new(),
                unicode: false,
            },
            package_flags: 0,
            name_count: 2,
            name_offset: 0,
            export_count: 0,
            export_offset: 0,
            import_count: 0,
            import_offset: 0,
            depends_offset: 0,
            unknown: [0; 4],
            guid: [guid; 16],
            generations: vec![],
            engine: 0,
            cooker: 0,
            compression: 0,
            summary_chunks: vec![],
            unknown5: 0,
            strings: vec![],
            unknown_records: vec![],
            garbage_size: 0,
            chunk_info_offset: 0,
            last_aes_block_size: 0,
            header_nonce: (licensee >= 33).then_some([0; 12]),
        };
        let size = s.write().len();
        s.name_offset = size as i32;
        let mut h = Writer::new();
        for text in [id.into(), format!("{id}_SF")] {
            FString {
                text,
                unicode: false,
            }
            .write(&mut h);
            h.u64(0)
        }
        h.i32(0);
        let actual = h.pos();
        while !h.pos().is_multiple_of(16) {
            h.raw(&[0])
        }
        s.total_header = (size + actual) as i32;
        crypt(&mut h.bytes, key, false);
        let mut out = s.write();
        out.extend(h.bytes);
        out.extend_from_slice(b"payload-data");
        out
    }
    fn ctr_fixture(id: &str, key: &[u8; 32]) -> Vec<u8> {
        let basic = fixture(id, key, 1, 34);
        let (mut s, _) = Summary::read(&basic).unwrap();
        let tables = read_tables(&basic, &s, key).unwrap();
        s.package_flags |= FULL_ENCRYPTION;
        s.header_nonce = Some([3; 12]);
        let verification = b"verification bytes after the tables";
        s.garbage_size = verification.len() as i32;
        let mut h = Writer::new();
        for n in tables.names {
            n.value.write(&mut h);
            h.u64(n.flags);
        }
        s.chunk_info_offset = h.pos() as i32;
        s.total_header = s.name_offset + (h.pos() + 4 + 2 * 36 + verification.len()) as i32;
        let mut offset = s.total_header as i64;
        let mut payload = Vec::new();
        let mut chunks = Vec::new();
        for (data, nonce) in [
            (b"first asset chunk".as_slice(), [4; 12]),
            (b"second chunk", [5; 12]),
        ] {
            chunks.push(Chunk {
                uncompressed_offset: offset,
                uncompressed_size: data.len() as i32,
                compressed_offset: offset,
                compressed_size: data.len() as i32,
                nonce: Some(nonce),
            });
            let mut encrypted = data.to_vec();
            crypt_ctr(&mut encrypted, key, &nonce).unwrap();
            payload.extend(encrypted);
            offset += data.len() as i64;
        }
        h.array(&chunks, |w, c| c.write(w));
        h.raw(verification);
        crypt_header(&mut h.bytes, &s, key, false).unwrap();
        let mut out = s.write();
        out.extend(h.bytes);
        out.extend(payload);
        out
    }

    #[test]
    fn ctr_matches_openssl_vector_including_partial_block() {
        // AES-256-CTR, zero key and IV. The second block checks counter order.
        let expected = [
            0xdc, 0x95, 0xc0, 0x78, 0xa2, 0x40, 0x89, 0x89, 0xad, 0x48, 0xa2, 0x14, 0x92, 0x84,
            0x20, 0x87, 0x53, 0x0f, 0x8a, 0xfb, 0xc7,
        ];
        let mut bytes = [0; 21];
        crypt_ctr(&mut bytes, &[0; 32], &[0; 12]).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rekeys_ctr_header_verification_and_every_asset_chunk() {
        let donor_key = [7; 32];
        let target_key = [9; 32];
        let donor = ctr_fixture("Boost_Dev", &donor_key);
        assert!(validate_identity(&donor, &target_key, "Boost_Dev").is_err());
        for target_id in ["A", "Boost_A_Much_Longer_Target"] {
            let target = fixture(target_id, &target_key, 2, 34);
            let result = masquerade(
                &donor,
                &target,
                &donor_key,
                &target_key,
                "Boost_Dev",
                target_id,
            )
            .unwrap();
            validate_identity(&result, &target_key, target_id).unwrap();
            let (s, _) = Summary::read(&result).unwrap();
            assert!(s.full_encryption());
            assert_eq!(s.guid, [2; 16]);
            let t = read_tables(&result, &s, &target_key).unwrap();
            let mut header = result[s.name_offset as usize..s.total_header as usize].to_vec();
            crypt_header(&mut header, &s, &target_key, true).unwrap();
            assert!(header.ends_with(b"verification bytes after the tables"));
            assert_eq!(t.chunks.len(), 2);
            for (chunk, expected) in t
                .chunks
                .iter()
                .zip([b"first asset chunk".as_slice(), b"second chunk"])
            {
                let start = chunk.compressed_offset as usize;
                let mut bytes = result[start..start + chunk.compressed_size as usize].to_vec();
                crypt_ctr(&mut bytes, &target_key, chunk.nonce.as_ref().unwrap()).unwrap();
                assert_eq!(bytes, expected);
            }
        }
    }

    #[test]
    fn supports_fully_encrypted_target_with_ecb_donor() {
        let a = [7; 32];
        let b = [9; 32];
        let donor = fixture("Boost_Standard", &a, 1, 34);
        let target = ctr_fixture("Boost_Dev", &b);
        let result = masquerade(&donor, &target, &a, &b, "Boost_Standard", "Boost_Dev").unwrap();
        validate_identity(&result, &b, "Boost_Dev").unwrap();
        assert!(!Summary::read(&result).unwrap().0.full_encryption());
        assert!(result.ends_with(b"payload-data"));
    }
    #[test]
    fn supports_longer_target_and_cross_version() {
        let a = [7; 32];
        let b = [9; 32];
        let donor = fixture("Hat_A", &a, 1, 32);
        let target = fixture("Hat_MuchLongerName", &b, 2, 34);
        let result = masquerade(&donor, &target, &a, &b, "Hat_A", "Hat_MuchLongerName").unwrap();
        validate_identity(&result, &b, "Hat_MuchLongerName").unwrap();
        assert_eq!(package_guid(&result).unwrap(), [2; 16]);
        assert_eq!(package_version(&result).unwrap(), (868, 32));
        assert!(result.ends_with(b"payload-data"))
    }
    #[test]
    fn rejects_wrong_key() {
        let key = [7; 32];
        let donor = fixture("Boost_AlphaReward", &key, 1, 34);
        let target = fixture("Boost_Standard", &key, 2, 34);
        assert!(
            masquerade(
                &donor,
                &target,
                &[0; 32],
                &key,
                "Boost_AlphaReward",
                "Boost_Standard"
            )
            .is_err()
        )
    }
    #[test]
    #[ignore = "requires a Rocket League install and downloaded key index"]
    fn validates_installed_alpha_to_standard_packages() {
        let root = std::path::PathBuf::from(std::env::var("RL_INSTALL").unwrap())
            .join("TAGame/CookedPCConsole");
        let csv = std::fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let key = |p: &str| {
            csv.lines()
                .skip(1)
                .find_map(|line| {
                    let f = line.split(',').collect::<Vec<_>>();
                    (f.len() >= 9 && f[7].trim_matches('"') == p)
                        .then(|| key_from_base64(f[8].trim_matches('"')).unwrap())
                })
                .unwrap()
        };
        let donor = std::fs::read(root.join("Boost_AlphaReward_SF.upk")).unwrap();
        let target = std::fs::read(root.join("Boost_Standard_SF.upk")).unwrap();
        let result = masquerade(
            &donor,
            &target,
            &key("Boost_AlphaReward"),
            &key("Boost_Standard"),
            "Boost_AlphaReward",
            "Boost_Standard",
        )
        .unwrap();
        validate_identity(&result, &key("Boost_Standard"), "Boost_Standard").unwrap()
    }
}
