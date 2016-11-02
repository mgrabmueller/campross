extern crate campross;
extern crate getopts;
extern crate ring;
extern crate mktemp;

use std::time::Instant;
use std::fs::File;
use std::io::{Write, Read};
use std::io::{BufReader, BufWriter};
use std::env;

use ring::digest;
use getopts::Options;
use mktemp::Temp;

use campross::arith;
use campross::witten_arith;
use campross::lzw;
use campross::lz77;
use campross::lzss;
use campross::huff;
use campross::lzp;
use campross::binarith;

#[derive(Debug,Clone,Copy)]
pub enum Method {
    Arith,
    WittenArith,
    Lzw,
    Lz77,
    Lzss,
    Huff,
    Lzp,
    BinArith,
}

fn do_compress(input: &str, output: &str, method: Method, stats: bool) {
    let _ = compress_with(input, output, method);

    if stats {
        let inf = File::open(input).unwrap();
        let outf = File::open(output).unwrap();
        let in_size =inf.metadata().unwrap().len();
        let out_size = outf.metadata().unwrap().len();
        println!("Original size: {}", in_size);
        println!("Compressed size: {}", out_size);
        println!("Ratio: {:.2}", out_size as f32 / in_size as f32);
    }
}

fn do_decompress(input: &str, output: &str, method: Method, _stats: bool) {
    let _ = decompress_with(input, output, method);
}

fn do_test(input: &str, method: Method) {
    let mut temp_dir = Temp::new_dir().unwrap();
    let mut compressed_name_buf = temp_dir.to_path_buf();
    compressed_name_buf.push("campross-test.compressed");
    let compressed_name = compressed_name_buf.as_path();
    
    let mut decompressed_name_buf = temp_dir.to_path_buf();
    decompressed_name_buf.push("campross-test.decompressed");
    let decompressed_name = decompressed_name_buf.as_path();

    let orig_hash = {
        println!("Calculating hash for input file {}...", input);
        let mut buf = [0u8; 1024 * 4];
        let mut ctx = digest::Context::new(&digest::SHA256);
        let mut inf = File::open(input).expect("cannot open input file");
        let mut nread = inf.read(&mut buf[..]).expect("cannot read input file");
        while nread > 0 {
            ctx.update(&buf[0..nread]);
            nread = inf.read(&mut buf[..]).expect("cannot read input file");
        }
        ctx.finish()
    };
    let start_compress = Instant::now();
    let (orig_size, compressed_size) =
        decompress_with(input, compressed_name.to_str().unwrap(), method);
    let compress_duration = start_compress.elapsed();

    let decompress_start = Instant::now();
    let (compressed_size2, decompressed_size) =
        decompress_with(input, compressed_name.to_str().unwrap(), method);
    let decompress_duration = decompress_start.elapsed();
    
    let decompressed_hash = {
        println!("Calculating hash for decompressed file {}...", decompressed_name.to_str().unwrap());
        let mut buf = [0u8; 1024 * 4];
        let mut ctx = digest::Context::new(&digest::SHA256);
        let mut inf = File::open(decompressed_name).expect("cannot open input file");
        let mut nread = inf.read(&mut buf[..]).expect("cannot read input file");
        while nread > 0 {
            ctx.update(&buf[0..nread]);
            nread = inf.read(&mut buf[..]).expect("cannot read input file");
        }
        ctx.finish()
    };
    assert_eq!(compressed_size, compressed_size2);

    let compress_secs = (compress_duration.as_secs() * 1_000 +
        compress_duration.subsec_nanos() as u64/ 1_000_000) as f64 / 1000.0;
    let decompress_secs = (decompress_duration.as_secs() * 1_000 +
        decompress_duration.subsec_nanos() as u64 / 1_000_000) as f64 / 1000.0;
    println!("Original size: {}", orig_size);
    println!("Compressed size: {}", compressed_size);
    println!("Ratio: {:.2}", compressed_size as f32 / orig_size as f32);
    println!("Compression speed: {:.3} MB/s", orig_size as f64 / compress_secs / (1024.0*1024.0));
    println!("Decompression speed: {:.3} MB/s", orig_size as f64 / decompress_secs / (1024.0*1024.0));


    if orig_size != decompressed_size {
        temp_dir.release();
        println!("ERROR: original and decompressed file differ in size");
    } else if orig_hash.as_ref() != decompressed_hash.as_ref() {
        temp_dir.release();
        println!("ERROR: original and decompressed file hashes differ");
    } else {
        println!("OK.");
    }
}

fn compress_with(input: &str, output: &str, method: Method) -> (u64, u64) {
    println!("Compressing {:?}...", method);
    {
        let inf = BufReader::new(File::open(input).unwrap());
        let outf = BufWriter::new(File::create(output).unwrap());

        let mut out = match method {
            Method::Arith => {
                let enc = arith::Encoder::new();
                enc.compress(inf, outf).unwrap()
            },
            Method::WittenArith => {
                witten_arith::compress(inf, outf).unwrap()
            },
            Method::Lzw => {
                lzw::compress(inf, outf).unwrap()
            },
            Method::Lz77 => {
                lz77::compress(inf, outf).unwrap()
            },
            Method::Lzss => {
                lzss::compress(inf, outf).unwrap()
            },
            Method::Huff => {
                huff::compress(inf, outf).unwrap()
            },
            Method::Lzp => {
                lzp::compress(inf, outf).unwrap()
            },
            Method::BinArith => {
                binarith::compress(inf, outf).unwrap()
            },
        };
        out.flush().unwrap();
    }
    
    let inf = File::open(input).unwrap();
    let outf = File::open(output).unwrap();
    let in_size = inf.metadata().unwrap().len();
    let out_size = outf.metadata().unwrap().len();
    (in_size, out_size)
}

fn decompress_with(input: &str, output: &str, method: Method) -> (u64, u64) {
    println!("Decompressing {:?}...", method);
    {
        let inf = BufReader::new(File::open(input).unwrap());
        let outf = BufWriter::new(File::create(output).unwrap());

        let mut out = match method {
            Method::Arith => {
                let enc = arith::Decoder::new();
                enc.decompress(inf, outf).unwrap()
            },
            Method::WittenArith => {
                witten_arith::decompress(inf, outf).unwrap()
            },
            Method::Lzw => {
                lzw::decompress(inf, outf).unwrap()
            },
            Method::Lz77 => {
                lz77::decompress(inf, outf).unwrap()
            },
            Method::Lzss => {
                lzss::decompress(inf, outf).unwrap()
            },
            Method::Huff => {
                huff::decompress(inf, outf).unwrap()
            },
            Method::Lzp => {
                lzp::decompress(inf, outf).unwrap()
            },
            Method::BinArith => {
                binarith::decompress(inf, outf).unwrap()
            },
        };
        out.flush().unwrap();
        
        let inf = File::open(input).unwrap();
        let outf = File::open(output).unwrap();
        let in_size = inf.metadata().unwrap().len();
        let out_size = outf.metadata().unwrap().len();
        (in_size, out_size)
    }
}

pub struct Result {
    pub input: String,
    pub method: Method,
    pub ratio: f64,
    pub orig_size: u64,
    pub compressed_size: u64,
    pub decompressed_size: u64,
    pub orig_hash: Vec<u8>,
    pub decompressed_hash: Vec<u8>,
    pub size_differ: bool,
    pub hash_differ: bool,
    pub compress_throughput: f64,
    pub decompress_throughput: f64,
}

fn do_compare(input: &str) {
    use Method::*;
    
    let temp_dir = Temp::new_dir().unwrap();
    let mut compressed_name_buf = temp_dir.to_path_buf();
    compressed_name_buf.push("campross-test.compressed");
    let compressed_name = compressed_name_buf.as_path();
    
    let mut decompressed_name_buf = temp_dir.to_path_buf();
    decompressed_name_buf.push("campross-test.decompressed");
    let decompressed_name = decompressed_name_buf.as_path();

    let orig_hash = {
        println!("Calculating hash for input file...");
        let mut buf = [0u8; 1024 * 4];
        let mut ctx = digest::Context::new(&digest::SHA256);
        let mut inf = File::open(input).expect("cannot open input file");
        let mut nread = inf.read(&mut buf[..]).expect("cannot read input file");
        while nread > 0 {
            ctx.update(&buf[0..nread]);
            nread = inf.read(&mut buf[..]).expect("cannot read input file");
        }
        ctx.finish()
    };
        let orig_hash_vec = orig_hash.as_ref().to_vec();

    let mut results: Vec<Result> = Vec::new();
    for method in [Arith, BinArith, WittenArith, Lzw, Lz77, Lzss, Lzp,
                   Huff].iter() {
        let start_compress = Instant::now();
        let (orig_size, compressed_size) =
            compress_with(input, compressed_name.to_str().unwrap(), *method);
        let compress_duration = start_compress.elapsed();

        let decompress_start = Instant::now();
        let (compressed_size2, decompressed_size) =
            decompress_with(compressed_name.to_str().unwrap(),
                            decompressed_name.to_str().unwrap(),
                            *method);
        let decompress_duration = decompress_start.elapsed();
    
        let decompressed_hash = {
            println!("Calculating hash for decompressed file...");
            let mut buf = [0u8; 1024 * 4];
            let mut ctx = digest::Context::new(&digest::SHA256);
            let mut inf = File::open(decompressed_name).expect("cannot open input file");
            let mut nread = inf.read(&mut buf[..]).expect("cannot read input file");
            while nread > 0 {
                ctx.update(&buf[0..nread]);
                nread = inf.read(&mut buf[..]).expect("cannot read input file");
            }
            ctx.finish()
        };
        assert_eq!(compressed_size, compressed_size2);

        let compress_secs = (compress_duration.as_secs() * 1_000 +
                             compress_duration.subsec_nanos() as u64/ 1_000_000) as f64 / 1000.0;
        let decompress_secs = (decompress_duration.as_secs() * 1_000 +
                               decompress_duration.subsec_nanos() as u64 / 1_000_000) as f64 / 1000.0;

        let compress_throughput = orig_size as f64 / compress_secs / (1024.0*1024.0);
        let decompress_throughput = orig_size as f64 / decompress_secs / (1024.0*1024.0);
        let decompressed_hash_vec = decompressed_hash.as_ref().to_vec();
        let hash_differ = orig_hash_vec != decompressed_hash_vec;
        let ratio = compressed_size as f64 / orig_size as f64;
        let result =
            Result {
                input: input.to_string(),
                method: *method,
                ratio: ratio,
                orig_size: orig_size,
                compressed_size: compressed_size,
                decompressed_size: decompressed_size,
                orig_hash: orig_hash_vec.clone(),
                decompressed_hash: decompressed_hash_vec,
                size_differ: orig_size != decompressed_size,
                hash_differ: hash_differ,
                compress_throughput: compress_throughput,
                decompress_throughput: decompress_throughput,
            };

        results.push(result);
    }

    println!("{:20} {:>8} {:>8} {:>8} {:>8} {:>8} {:11} {:6}",
             "Filename", "Orig.Sz.", "Cmp.Sz.", "Ratio", "Cmp.Spd", "Dec.Spd", "Method", "Check");
    for res in results {
        let result =
            if res.size_differ || res.hash_differ {
                "ERROR"
            } else {
                "OK"
            };
        print!("{:20} {:8} {:8} {:8.2} {:8.2} {:8.2} {:11} {:6}",
               input,
               res.orig_size, res.compressed_size,
               res.ratio, res.compress_throughput, res.decompress_throughput,
               format!("{:?}", res.method), result);
        println!("");
    }
}

/// Print a usage summary to stdout that describes the command syntax.
fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} FILE", program);
    print!("{}", opts.usage(&brief));
}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("i", "input", "set input file", "FILE");
    opts.optopt("o", "output", "set output file", "FILE");
    opts.optflag("c", "compress", "compress the input file");
    opts.optflag("d", "decompress", "decompress the input file");
    opts.optflag("t", "test", "test compressor on a file");
    opts.optflag("p", "compare", "compare all compressors on a file");
    opts.optopt("m", "method", "select compression method", "arith|warith|lzw|lz77|lzss|lzmg2|huff|lzp|binarith");
    opts.optflag("s", "stats", "print statistics");
    opts.optflag("h", "help", "print this help");

    match opts.parse(&args[1..]) {
        Ok(matches) => {
            if matches.opt_present("h") {
                print_usage(&program, &opts);
            }
            let method =
                if let Some(s) = matches.opt_str("m") {
                    match &s[..] {
                        "arith" => Some(Method::Arith),
                        "warith" => Some(Method::WittenArith),
                        "lzw"   => Some(Method::Lzw),
                        "lz77"   => Some(Method::Lz77),
                        "lzss"   => Some(Method::Lzss),
                        "huff"  => Some(Method::Huff),
                        "lzp"  => Some(Method::Lzp),
                        "binarith"  => Some(Method::BinArith),
                        _       => None,
                    }
                } else {
                    Some(Method::Arith)
                };
            if matches.opt_present("t") {
                if let Some(m) = method {
                    match matches.opt_str("i") {
                        Some(input) => {
                            do_test(&input, m);
                        },
                        None => {
                            print_usage(&program, &opts);
                        }
                    }
                } else {
                    print_usage(&program, &opts);
                }
            } else if matches.opt_present("p") {
                match matches.opt_str("i") {
                    Some(input) => {
                        do_compare(&input);
                    },
                    None => {
                        print_usage(&program, &opts);
                    }
                }
            } else {
                match (matches.opt_str("i"), matches.opt_str("o")) {
                    (Some(input), Some(output)) => {
                        let stats = matches.opt_present("s");
                        match (method, matches.opt_present("c"), matches.opt_present("d")) {
                            (Some(m), true, false) => {
                                do_compress(&input, &output, m, stats);
                            },
                            (Some(m), false, true) => {
                                do_decompress(&input, &output, m, stats);
                            },
                            _ => {
                                print_usage(&program, &opts);
                            },
                        }
                    },
                    _ =>
                        print_usage(&program, &opts),
                }
            }
        },
        Err(e) => {
            println!("Error: {}", e);
            print_usage(&program, &opts);
        },
    }

}
