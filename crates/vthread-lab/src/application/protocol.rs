//! Bounded framed requests and deterministic binary responses.

use vthread::{Result, net::TcpStream};

pub(crate) const MAX_INPUT: usize = 4096;
pub(crate) const MAX_OUTPUT: usize = 65536;
pub(crate) const READY: u8 = 1;
pub(crate) const BUSY: u8 = 2;

pub(crate) fn lengths(header: &[u8; 16]) -> Result<(usize, usize, u64)> {
    let input = u32::from_be_bytes(header[..4].try_into().expect("header")) as usize;
    let output = u32::from_be_bytes(header[4..8].try_into().expect("header")) as usize;
    let sequence = u64::from_be_bytes(header[8..].try_into().expect("header"));
    if input == 0 || input > MAX_INPUT || output == 0 || output > MAX_OUTPUT {
        return Err(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "frame limits exceeded").into(),
        );
    }
    Ok((input, output, sequence))
}

pub(crate) fn response(input: &[u8], output: usize, sequence: u64) -> Vec<u8> {
    (0..output)
        .map(|index| input[index % input.len()] ^ (sequence.wrapping_add(index as u64) as u8))
        .collect()
}

pub(crate) fn exchange(stream: &TcpStream) -> Result<bool> {
    let mut header = [0; 16];
    if stream.read(&mut header[..1])? == 0 {
        return Ok(false);
    }
    stream.read_exact(&mut header[1..])?;
    let (input, output, sequence) = lengths(&header)?;
    let mut bytes = vec![0; input];
    stream.read_exact(&mut bytes)?;
    stream.write_all(&header[8..])?;
    stream.write_all(&response(&bytes, output, sequence))?;
    Ok(true)
}

#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;
