extern crate campross;
extern crate getopts;
extern crate ring;

use std::time::Instant;
use ring::digest;
use getopts::Options;
use std::fs::File;
use std::io::{Write, Read};
use std::io::{BufReader, BufWriter};
use std::env;

use campross::arith;
use campross::lzw;
use campross::lzmg1;
use campross::lzmg2;
use campross::huff;

#[derive(Debug)]
enum Method {
    Arith,
    Lzw,
    Lzmg1,
    Lzmg2,
    Huff,
}

fn do_compress(input: &str, output: &str, method: Method, stats: bool) {
    {
        let inf = File::open(input).unwrap();
        let outf = File::create(output).unwrap();

        let mut out = match method {
            Method::Arith => {
                let enc = arith::Encoder::new();
                enc.compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Lzw => {
                lzw::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Lzmg1 => {
                lzmg1::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Lzmg2 => {
                lzmg2::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Huff => {
                huff::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
        };
        out.flush().unwrap();
    }

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
    let inf = File::open(input).unwrap();
    let outf = File::create(output).unwrap();

    let mut out = match method {
        Method::Arith => {
            let enc = arith::Decoder::new();
            enc.decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Lzw => {
            lzw::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Lzmg1 => {
            lzmg1::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Lzmg2 => {
            lzmg2::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Huff => {
            huff::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
    };
    out.flush().unwrap();
}

fn do_inspect(input: &str, method: Method) {
    let inf = File::open(input).unwrap();

    match method {
        Method::Arith => {
            println!("inspect mode not supported for arithmetic encoder");
        },
        Method::Lzw => {
            lzw::inspect(BufReader::new(inf)).unwrap();
        },
        Method::Lzmg1 => {
            println!("inspect mode not supported for LZMG1 encoder");
        },
        Method::Lzmg2 => {
            println!("inspect mode not supported for LZMG2 encoder");
        },
        Method::Huff => {
            println!("inspect mode not supported for Huffman encoder");
        },
    }
}

fn do_test(input: &str, method: Method) {
    let compressed_name = "/tmp/campross-test.compressed";
    let decompressed_name = "/tmp/campross-test.decompressed";
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
    {
        println!("Compressing {} to {} (method: {:?})...", input, compressed_name,
                 method);
        {
            let inf = File::open(input).unwrap();
            let outf = File::create(compressed_name).unwrap();

            let mut out = match method {
                Method::Arith => {
                    let enc = arith::Encoder::new();
                    enc.compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzw => {
                    lzw::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzmg1 => {
                    lzmg1::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzmg2 => {
                    lzmg2::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Huff => {
                    huff::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
            };
            out.flush().unwrap();
        }
        
        let inf = File::open(input).unwrap();
        let outf = File::open(compressed_name).unwrap();
        let in_size = inf.metadata().unwrap().len();
        let out_size = outf.metadata().unwrap().len();
        (in_size, out_size)
    };
    let compress_duration = start_compress.elapsed();

    let decompress_start = Instant::now();
    let (compressed_size2, decompressed_size) =
    {
        println!("Decompressing {} to {} (method: {:?})...", compressed_name,
                 decompressed_name,
                 method);
        {
            let inf = File::open(compressed_name).unwrap();
            let outf = File::create(decompressed_name).unwrap();

            let mut out = match method {
                Method::Arith => {
                    let enc = arith::Decoder::new();
                    enc.decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzw => {
                    lzw::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzmg1 => {
                    lzmg1::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Lzmg2 => {
                    lzmg2::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
                Method::Huff => {
                    huff::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
                },
            };
            out.flush().unwrap();
        }
        
        let inf = File::open(compressed_name).unwrap();
        let outf = File::open(decompressed_name).unwrap();
        let in_size = inf.metadata().unwrap().len();
        let out_size = outf.metadata().unwrap().len();
        (in_size, out_size)
    };
    let decompress_duration = decompress_start.elapsed();
    
    let decompressed_hash = {
        println!("Calculating hash for decompressed file {}...", decompressed_name);
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

    println!("Original size: {}", orig_size);
    println!("Compressed size: {}", compressed_size);
    println!("Ratio: {:.2}", compressed_size as f32 / orig_size as f32);
    println!("Compression speed: {} MB/s", orig_size as f32 / (compress_duration.as_secs() as f32) / (1024.0*1024.0));
    println!("Decompression speed: {} MB/s", orig_size as f32 / (decompress_duration.as_secs() as f32) / (1024.0*1024.0));


    if orig_size != decompressed_size {
        println!("ERROR: original and decompressed file differ in size");
    } else if orig_hash.as_ref() != decompressed_hash.as_ref() {
        println!("ERROR: original and decompressed file hashes differ");
    } else {
        println!("OK.");
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
    opts.optflag("x", "examine", "examine a compressed file");
    opts.optflag("t", "test", "test compressor on a file");
    opts.optopt("m", "method", "select compression method", "arith|lzw|lzmg1|lzmg2|huff");
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
                        "lzw"   => Some(Method::Lzw),
                        "lzmg1"  => Some(Method::Lzmg1),
                        "lzmg2"  => Some(Method::Lzmg2),
                        "huff"  => Some(Method::Huff),
                        _       => None,
                    }
                } else {
                    Some(Method::Arith)
                };
            if matches.opt_present("x") {
                if let Some(m) = method {
                    match matches.opt_str("i") {
                        Some(input) => {
                            do_inspect(&input, m);
                        },
                        None => {
                            print_usage(&program, &opts);
                        }
                    }
                } else {
                    print_usage(&program, &opts);
                }
            } else if matches.opt_present("t") {
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
