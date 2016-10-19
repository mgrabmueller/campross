use std::collections::HashMap;
use std::io::{Read, Write};
use error;
use bitfile::{BitWriter, BitReader};

const EOF: usize = 256;

struct Entry {
    code: usize,
}

pub fn compress<R, W>(mut input: R, output: W) -> Result<W, error::Error>
    where R: Read, W: Write {
    let max_code_len = 16;
    let max_code = (1 << max_code_len) - 1;
    let mut code_len = 9;
    let mut next_code = 257;
    let mut dict: HashMap<Vec<u8>, Entry> = HashMap::new();
    for c in 0..256 {
        let mut s = Vec::new();
        s.push(c as u8);
        dict.insert(s, Entry{code: c});
    }

    let mut current_string: Vec<u8> = Vec::new();

    let mut out = BitWriter::new(output);
    let mut buf = [0u8; 1];
    
    let mut nread = try!(input.read(&mut buf));
    while nread == 1 {
        let c = buf[0];

        current_string.push(c);
        if let None = dict.get(&current_string) {
            if next_code <= max_code {
                dict.insert(current_string.clone(),
                            Entry{code: next_code});
                next_code += 1;
            }
            let _ = current_string.pop();
            if let Some(entry) = dict.get(&current_string) {
                try!(out.write_bits(entry.code, code_len));
            } else {
                unreachable!();
            }
            current_string.truncate(0);
            current_string.push(c);
            if next_code < max_code && next_code >= (1 << code_len) {
                code_len += 1;
            }
        }
            
        nread = try!(input.read(&mut buf));
    }
    
    if current_string.len() > 0 {
        if let Some(entry) = dict.get(&current_string) {
            try!(out.write_bits(entry.code, code_len));
        } else {
            unreachable!();
        }
    }

    try!(out.write_bits(EOF, code_len));
    out.flush()
}

pub fn decompress<R, W>(input: R, mut output: W) -> Result<W, error::Error>
    where R: Read, W: Write {
    let max_code_len = 16;
    let max_code = (1 << max_code_len) - 1;
    let mut code_len = 9;
    let mut next_code = 257;
    let mut dict: HashMap<usize, Vec<u8>> = HashMap::new();
    for c in 0..256 {
        let mut s = Vec::new();
        s.push(c as u8);
        dict.insert(c, s);
    }

    let mut previous_string: Vec<u8> = Vec::new();

    let mut inp = BitReader::new(input);

    let mut code = try!(inp.read_bits(code_len)) as usize;
    while code != EOF {
        if let None = dict.get(&code) {
            let mut s = Vec::new();
            s.extend_from_slice(&previous_string[..]);
            s.extend_from_slice(&previous_string[0..1]);
            dict.insert(code, s);
        }

        let str_code = dict.get(&code).unwrap().clone();
        let _ = try!(output.write(&str_code[..]));
        
        if previous_string.len() > 0 && next_code <= max_code {
            let mut ns = Vec::new();
            ns.extend_from_slice(&previous_string[..]);
            ns.extend_from_slice(&str_code[0..1]);
            dict.insert(next_code, ns);
            next_code += 1;
        }
        previous_string = str_code;

        if next_code < max_code && next_code + 1 >= (1 << code_len) {
            code_len += 1;
        }
        code = try!(inp.read_bits(code_len)) as usize;

    }
    
    Ok(output)
}

#[cfg(test)]
mod test {
    use ::std::io::Cursor;
    use super::{compress, decompress};

    #[test]
    fn compress_empty() {
        let input = b"";
        let expected = [128, 0];
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"A";
        let expected = [32, 192, 0];
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_aaa() {
        let input = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let expected = [32, 192, 96, 80, 56, 36, 22, 13, 7, 130, 192, 0];
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_empty() {
        let input = [128, 0];
        let expected = b"";
        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_a() {
        let input = [32, 192, 0];
        let expected = b"A";
        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_aaa() {
        let input = [32, 192, 96, 80, 56, 36, 22, 13, 7, 130, 192, 0];
        let expected = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        use ::std::io::Cursor;
        let f = include_bytes!("lzw.rs");
        let original = &f[..];
        
        let compressed = compress(Cursor::new(&original[..]), vec![]).unwrap();
        
        let decompressed = decompress(Cursor::new(&compressed[..]), vec![]).unwrap();
        assert_eq!(original.len(), decompressed.len());
        assert_eq!(&original[..], &decompressed[..]);
    }
}
