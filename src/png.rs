//! Encodeur PNG minimal (RVB 8 bits, blocs deflate non compressés).
//! Suffisant pour les images de contrôle, sans dépendance externe.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
const MAX_STORED_BLOCK: usize = 65_535;

/// Image RVB, pixels rangés ligne par ligne depuis le coin haut-gauche.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 3]>,
}

impl RgbImage {
    pub fn new(width: usize, height: usize, background: [u8; 3]) -> Self {
        Self {
            width,
            height,
            pixels: vec![background; width * height],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, color: [u8; 3]) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color;
        }
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut png = SIGNATURE.to_vec();
        write_chunk(&mut png, b"IHDR", &self.header());
        write_chunk(&mut png, b"IDAT", &zlib_stored(&self.scanlines()));
        write_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn header(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(13);
        header.extend((self.width as u32).to_be_bytes());
        header.extend((self.height as u32).to_be_bytes());
        header.extend([8, 2, 0, 0, 0]); // 8 bits, RVB, deflate, filtre standard, pas d'entrelacement
        header
    }

    /// Chaque ligne est précédée de l'octet de filtre 0 (aucun filtre).
    fn scanlines(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity(self.height * (self.width * 3 + 1));
        for row in self.pixels.chunks(self.width) {
            raw.push(0);
            raw.extend(row.iter().flatten());
        }
        raw
    }
}

fn write_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend(kind);
    png.extend(data);
    let crc = crc32(&png[start..]);
    png.extend(crc.to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = data.chunks(MAX_STORED_BLOCK).collect();
    let last = blocks.len().saturating_sub(1);
    for (index, block) in blocks.iter().enumerate() {
        out.push(u8::from(index == last));
        let len = block.len() as u16;
        out.extend(len.to_le_bytes());
        out.extend((!len).to_le_bytes());
        out.extend(*block);
    }
    if blocks.is_empty() {
        out.extend([1, 0, 0, 0xFF, 0xFF]);
    }
    out.extend(adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_reference_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_starts_with_signature_and_ends_with_iend() {
        let png = RgbImage::new(3, 2, [255, 0, 0]).to_png();
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
