use std::io::{self, Write};
use std::path::Path;

use crate::store::SAMPLE_RATE;

/// Zapisuje mono PCM 16-bit (jak `soundfile.write` dla float32) do `w`. Próbki spoza `[-1, 1]` są obcinane.
pub fn encode_wav<W: Write>(w: &mut W, samples: &[f32], sample_rate: u32) -> io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut hdr = Vec::with_capacity(44);
    hdr.extend_from_slice(b"RIFF");
    hdr.extend_from_slice(&(36 + data_len).to_le_bytes());
    hdr.extend_from_slice(b"WAVEfmt ");
    hdr.extend_from_slice(&16u32.to_le_bytes()); // rozmiar bloku fmt
    hdr.extend_from_slice(&1u16.to_le_bytes()); // PCM
    hdr.extend_from_slice(&1u16.to_le_bytes()); // mono
    hdr.extend_from_slice(&sample_rate.to_le_bytes());
    hdr.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // bajty/s
    hdr.extend_from_slice(&2u16.to_le_bytes()); // block align
    hdr.extend_from_slice(&16u16.to_le_bytes()); // bity na próbkę
    hdr.extend_from_slice(b"data");
    hdr.extend_from_slice(&data_len.to_le_bytes());
    w.write_all(&hdr)?;
    let mut buf = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = f64::from(s).clamp(-1.0, 1.0);
        buf.extend_from_slice(&((v * 32767.0).round() as i16).to_le_bytes());
    }
    w.write_all(&buf)
}

/// Skrót: `encode_wav` do pliku z częstotliwością `SAMPLE_RATE`.
pub fn write_wav(path: impl AsRef<Path>, samples: &[f32]) -> io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    encode_wav(&mut f, samples, SAMPLE_RATE)?;
    f.flush()
}
