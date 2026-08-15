pub fn hash16(s: &str, histart: u16, lostart: u16) -> u16 {
    let mut hi = histart;
    let mut lo = lostart;

    for byte in s.bytes() {
        let b = byte as u16;
        lo = (b + lo) % 255;
        hi = ((255 - b) + hi) % 255;

        let lolo = lo % 16;
        let lohi = lo / 16;
        let hilo = hi % 16;
        let hihi = hi / 16;

        lo = (hilo * 16) + lolo;
        hi = (hihi * 16) + lohi;

        (lo, hi) = (hi, lo);
    }

    (hi * 255) + lo
}

pub fn hash32(s: &str) -> (u16, u16) {
    let hi = hash16(s, 170, 204);
    let lo = hash16(s, 11, 252);
    (hi, lo)
}

pub fn hash32str(s: &str) -> String {
    let (hi, lo) = hash32(s);
    format!("{:04x}{:04x}", hi, lo)
}
