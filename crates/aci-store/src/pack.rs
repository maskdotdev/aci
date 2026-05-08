use aci_core::{AciError, Result};
use std::io::{Read, Write};

const PACK_MAGIC: &[u8] = b"ACIPACK1\n";

pub(crate) type CompactSpan = [u32; 6];

pub(crate) fn write_pack_header(writer: &mut impl Write) -> Result<()> {
    writer.write_all(PACK_MAGIC)?;
    Ok(())
}

pub(crate) fn read_pack_header(reader: &mut impl Read) -> Result<()> {
    let mut magic = [0; PACK_MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if magic != PACK_MAGIC {
        return Err(AciError::Message(
            "partition pack has invalid header".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn write_string(writer: &mut impl Write, value: &str) -> Result<()> {
    write_len(writer, value.len(), "string")?;
    writer.write_all(value.as_bytes())?;
    Ok(())
}

pub(crate) fn read_string(reader: &mut impl Read) -> Result<String> {
    let len = read_var_u32(reader, "string length")?;
    let mut bytes = vec![0; capacity(len, "string")?];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes)
        .map_err(|error| AciError::Message(format!("partition pack has invalid utf-8: {error}")))
}

pub(crate) fn write_opt_span(writer: &mut impl Write, span: Option<CompactSpan>) -> Result<()> {
    match span {
        Some(span) => {
            write_u8(writer, 1)?;
            for value in span {
                write_var_u32(writer, value)?;
            }
        }
        None => write_u8(writer, 0)?,
    }
    Ok(())
}

pub(crate) fn read_opt_span(reader: &mut impl Read) -> Result<Option<CompactSpan>> {
    match read_u8(reader, "span presence")? {
        0 => Ok(None),
        1 => Ok(Some([
            read_var_u32(reader, "span byte start")?,
            read_var_u32(reader, "span byte end")?,
            read_var_u32(reader, "span start line")?,
            read_var_u32(reader, "span start column")?,
            read_var_u32(reader, "span end line")?,
            read_var_u32(reader, "span end column")?,
        ])),
        value => Err(AciError::Message(format!(
            "partition pack has invalid span presence tag {value}"
        ))),
    }
}

pub(crate) fn write_opt_u32(writer: &mut impl Write, value: Option<u32>) -> Result<()> {
    write_var_u32(writer, value.map(|value| value + 1).unwrap_or(0))
}

pub(crate) fn read_opt_u32(reader: &mut impl Read, field: &str) -> Result<Option<u32>> {
    Ok(match read_var_u32(reader, field)? {
        0 => None,
        value => Some(value - 1),
    })
}

pub(crate) fn write_opt_u8(writer: &mut impl Write, value: Option<u8>) -> Result<()> {
    write_u8(writer, value.unwrap_or(u8::MAX))
}

pub(crate) fn read_opt_u8(reader: &mut impl Read, field: &str) -> Result<Option<u8>> {
    Ok(match read_u8(reader, field)? {
        u8::MAX => None,
        value => Some(value),
    })
}

pub(crate) fn write_len(writer: &mut impl Write, len: usize, field: &str) -> Result<()> {
    let len = u32::try_from(len)
        .map_err(|_| AciError::Message(format!("partition pack has too many {field}")))?;
    write_var_u32(writer, len)
}

pub(crate) fn capacity(len: u32, field: &str) -> Result<usize> {
    usize::try_from(len).map_err(|_| {
        AciError::Message(format!(
            "partition pack {field} length does not fit this platform"
        ))
    })
}

pub(crate) fn write_u8(writer: &mut impl Write, value: u8) -> Result<()> {
    writer.write_all(&[value])?;
    Ok(())
}

pub(crate) fn read_u8(reader: &mut impl Read, field: &str) -> Result<u8> {
    let mut byte = [0; 1];
    reader
        .read_exact(&mut byte)
        .map_err(|error| truncated(error, field))?;
    Ok(byte[0])
}

pub(crate) fn write_var_u32(writer: &mut impl Write, value: u32) -> Result<()> {
    write_var_u64(writer, u64::from(value))
}

pub(crate) fn read_var_u32(reader: &mut impl Read, field: &str) -> Result<u32> {
    let value = read_var_u64_optional(reader)?
        .ok_or_else(|| AciError::Message(format!("partition pack ended before {field}")))?;
    u32::try_from(value)
        .map_err(|_| AciError::Message(format!("partition pack {field} does not fit u32")))
}

pub(crate) fn write_var_u64(writer: &mut impl Write, mut value: u64) -> Result<()> {
    while value >= 0x80 {
        writer.write_all(&[((value as u8) & 0x7f) | 0x80])?;
        value >>= 7;
    }
    writer.write_all(&[value as u8])?;
    Ok(())
}

pub(crate) fn read_var_u64(reader: &mut impl Read, field: &str) -> Result<u64> {
    read_var_u64_optional(reader)?
        .ok_or_else(|| AciError::Message(format!("partition pack ended before {field}")))
}

pub(crate) fn read_var_u64_optional(reader: &mut impl Read) -> Result<Option<u64>> {
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let mut byte = [0; 1];
        match reader.read(&mut byte) {
            Ok(0) if shift == 0 => return Ok(None),
            Ok(0) => {
                return Err(AciError::Message(
                    "partition pack has truncated varint".to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
        value |= u64::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(Some(value));
        }
        shift += 7;
        if shift >= 64 {
            return Err(AciError::Message(
                "partition pack varint is too large".to_string(),
            ));
        }
    }
}

fn truncated(error: std::io::Error, field: &str) -> AciError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        AciError::Message(format!("partition pack ended while reading {field}"))
    } else {
        error.into()
    }
}
