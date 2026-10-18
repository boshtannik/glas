use std::env;
use std::f64;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

const SIZES: [usize; 4] = [86, 108, 128, 172];
const SS: usize = 4;
const VIEW: f64 = 86.0;

type Pt = (f64, f64);

fn squircle_segments() -> Vec<(Pt, Pt, Pt, Pt)> {
    vec![
        ((43.0, 0.4), (19.417, 0.4), (0.3, 19.466), (0.3, 42.995)),
        ((0.3, 84.18), (0.3, 84.964), (0.937, 85.6), (1.723, 85.6)),
        ((43.0, 85.6), (66.583, 85.6), (85.7, 66.524), (85.7, 42.994)),
        ((85.7, 1.82), (85.7, 1.035), (85.063, 0.4), (84.277, 0.4)),
    ]
}

fn spark_polygon() -> Vec<Pt> {
    vec![
        (43.0, 13.0),
        (51.0, 35.0),
        (73.0, 43.0),
        (51.0, 51.0),
        (43.0, 73.0),
        (35.0, 51.0),
        (13.0, 43.0),
        (35.0, 35.0),
    ]
}

fn flatten(segments: &[(Pt, Pt, Pt, Pt)]) -> Vec<Pt> {
    const STEPS: usize = 32;
    let mut pts = Vec::new();
    let mut cur = segments[0].0;
    pts.push(cur);
    for seg in segments {
        let (p0, c1, c2, p3) = *seg;
        debug_assert_eq!(p0, cur);
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            let x = u * u * u * p0.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0
                + t * t * t * p3.0;
            let y = u * u * u * p0.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1
                + t * t * t * p3.1;
            pts.push((x, y));
        }
        cur = p3;
    }
    pts
}

fn in_polygon(pt: Pt, poly: &[Pt]) -> bool {
    let (x, y) = pt;
    let n = poly.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y)
            && x < (xj - xi) * (y - yi) / (yj - yi) + xi
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct Rng(u64);

impl Rng {
    fn new(seed: Option<u64>) -> Self {
        let s = seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        });
        Rng(if s == 0 { 0x9e3779b97f4a7c15 } else { s })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }
}

fn hls_to_rgb(h: f64, l: f64, s: f64) -> [u8; 3] {
    let f = |m1: f64, m2: f64, mut h: f64| {
        if h < 0.0 {
            h += 1.0;
        }
        if h > 1.0 {
            h -= 1.0;
        }
        if h < 1.0 / 6.0 {
            m1 + (m2 - m1) * h * 6.0
        } else if h < 0.5 {
            m2
        } else if h < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - h) * 6.0
        } else {
            m1
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let m1 = 2.0 * l - m2;
        (
            f(m1, m2, h + 1.0 / 3.0),
            f(m1, m2, h),
            f(m1, m2, h - 1.0 / 3.0),
        )
    };
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}

fn random_color(rng: &mut Rng) -> [u8; 3] {
    hls_to_rgb(rng.f64(), rng.range(0.38, 0.58), rng.range(0.55, 0.95))
}

fn random_gradient(rng: &mut Rng) -> ([u8; 3], [u8; 3]) {
    loop {
        let c1 = random_color(rng);
        let c2 = random_color(rng);
        let dist: f64 = (0..3)
            .map(|i| (c1[i] as f64 - c2[i] as f64).powi(2))
            .sum();
        if dist.sqrt() > 90.0 {
            return (c1, c2);
        }
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn build_svg(c1: [u8; 3], c2: [u8; 3]) -> String {
    let (c1, c2) = (hex(c1), hex(c2));
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="86" height="86" viewBox="0 0 86 86">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="86" y2="86" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="{c1}"/>
      <stop offset="1" stop-color="{c2}"/>
    </linearGradient>
  </defs>
  <path d="M84.277,0.4H43c-23.583,0,-42.7,19.066,-42.7,42.595V84.18c0,0.784,0.637,1.42,1.423,1.42H43c23.583,0,42.7,-19.076,42.7,-42.606V1.82c0,-0.785,-0.637,-1.42,-1.423,-1.42z" fill="url(#bg)"/>
  <path d="M43,13 L51,35 L73,43 L51,51 L43,73 L35,51 L13,43 L35,35 Z" fill="#ffffff"/>
</svg>
"##
    )
}

fn render(c1: [u8; 3], c2: [u8; 3], size: usize) -> Vec<u8> {
    let squircle = flatten(&squircle_segments());
    let spark = spark_polygon();
    let scale = size as f64 / VIEW;
    let inv = 1.0 / (SS * SS) as f64;
    let mut out = vec![0u8; size * size * 4];

    for py in 0..size {
        for px in 0..size {
            let mut a_sum = 0.0f64;
            let mut r_sum = 0.0f64;
            let mut g_sum = 0.0f64;
            let mut b_sum = 0.0f64;
            for sy in 0..SS {
                for sx in 0..SS {
                    // pixel → view(0..86) coordinate, sample at supersubpixel center
                    let x = ((px * SS + sx) as f64 + 0.5) / (SS as f64 * scale);
                    let y = ((py * SS + sy) as f64 + 0.5) / (SS as f64 * scale);
                    if !in_polygon((x, y), &squircle) {
                        continue;
                    }
                    let t = ((x + y) / (2.0 * VIEW)).clamp(0.0, 1.0);
                    let (r, g, b) = if in_polygon((x, y), &spark) {
                        (255.0, 255.0, 255.0)
                    } else {
                        (
                            c1[0] as f64 + (c2[0] as f64 - c1[0] as f64) * t,
                            c1[1] as f64 + (c2[1] as f64 - c1[1] as f64) * t,
                            c1[2] as f64 + (c2[2] as f64 - c1[2] as f64) * t,
                        )
                    };
                    a_sum += 1.0;
                    r_sum += r;
                    g_sum += g;
                    b_sum += b;
                }
            }
            let o = (py * size + px) * 4;
            if a_sum > 0.0 {
                out[o] = (r_sum * inv) as u8;
                out[o + 1] = (g_sum * inv) as u8;
                out[o + 2] = (b_sum * inv) as u8;
                out[o + 3] = (a_sum * inv * 255.0) as u8;
            }
        }
    }
    out
}

fn write_png(data: &[u8], size: usize, path: &Path) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut enc = png::Encoder::new(file, size as u32, size as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(data).map_err(|e| e.to_string())
}

fn usage() -> ! {
    eprintln!("usage: gen-icon [--out DIR] [--svg PATH] [--seed N]");
    process::exit(2);
}

fn main() {
    let mut out_dir = PathBuf::from("icons");
    let mut svg_path = PathBuf::from("harbour-glas.svg");
    let mut seed: Option<u64> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out_dir = PathBuf::from(args.next().unwrap_or_else(|| usage())),
            "--svg" => svg_path = PathBuf::from(args.next().unwrap_or_else(|| usage())),
            "--seed" => {
                seed = Some(
                    args.next()
                        .unwrap_or_else(|| usage())
                        .parse()
                        .unwrap_or_else(|_| usage()),
                )
            }
            _ => usage(),
        }
    }

    let mut rng = Rng::new(seed);
    let (c1, c2) = random_gradient(&mut rng);
    let svg = build_svg(c1, c2);

    if let Some(parent) = svg_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).unwrap_or_else(|e| {
                eprintln!("{}: {}", parent.display(), e);
                process::exit(1);
            });
        }
    }
    fs::write(&svg_path, &svg).unwrap_or_else(|e| {
        eprintln!("{}: {}", svg_path.display(), e);
        process::exit(1);
    });
    println!("svg: {}  gradient {} -> {}", svg_path.display(), hex(c1), hex(c2));

    for &size in &SIZES {
        let dir = out_dir.join(format!("{size}x{size}"));
        fs::create_dir_all(&dir).unwrap_or_else(|e| {
            eprintln!("{}: {}", dir.display(), e);
            process::exit(1);
        });
        let png_path = dir.join("harbour-glas.png");
        let data = render(c1, c2, size);
        write_png(&data, size, &png_path).unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        });
        println!("png: {}", png_path.display());
    }
}
