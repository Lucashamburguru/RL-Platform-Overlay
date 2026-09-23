//! Rocket League UPK package identity rewriting.

use aes::Aes256;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use base64::Engine as _;

const MAGIC: u32 = 0x9e2a83c1;

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
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
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
}
impl Summary {
    fn modern(&self) -> bool {
        self.licensee >= 33
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
        Ok(((n as usize) + 15) & !15)
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
        if name_count < 0
            || export_count < 0
            || import_count < 0
            || name_offset < r.pos as i32
            || total_header < name_offset
            || total_header as usize > bytes.len()
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
    for raw in bytes.chunks_exact_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(raw);
        if decrypt {
            cipher.decrypt_block(block)
        } else {
            cipher.encrypt_block(block)
        }
    }
}
fn read_tables(bytes: &[u8], s: &Summary, key: &[u8; 32]) -> Result<Tables, String> {
    let len = s.encrypted_len()?;
    let start = s.name_offset as usize;
    let mut plain = bytes
        .get(start..start + len)
        .ok_or("Truncated encrypted UPK header")?
        .to_vec();
    crypt(&mut plain, key, true);
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

pub fn masquerade(
    donor: &[u8],
    target: &[u8],
    donor_key: &[u8; 32],
    target_key: &[u8; 32],
    donor_id: &str,
    target_id: &str,
) -> Result<Vec<u8>, String> {
    let (mut ds, summary_len) = Summary::read(donor)?;
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
    let padded = (unpadded + 15) & !15;
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
    crypt(&mut header.bytes, target_key, false);
    let payload_start = Summary::read(donor)?.0.name_offset as usize + old_encrypted as usize;
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
        while h.pos() % 16 != 0 {
            h.raw(&[0])
        }
        s.total_header = (size + actual) as i32;
        crypt(&mut h.bytes, key, false);
        let mut out = s.write();
        out.extend(h.bytes);
        out.extend_from_slice(b"payload-data");
        out
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
