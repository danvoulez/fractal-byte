use anyhow::Result;
use rb_vm::tlv::decode_stream;
use rb_vm::opcode::Opcode;
use std::fs;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).expect("usage: rb_vm_disasm <chip.tlv>");
    let buf = fs::read(path)?;
    let code = decode_stream(&buf)?;
    for (i, ins) in code.iter().enumerate() {
        match ins.op {
            Opcode::ConstI64 => {
                let v = i64::from_be_bytes(ins.payload.try_into().unwrap());
                println!("{:04}: ConstI64 {}", i, v);
            }
            Opcode::PushInput => {
                let idx = u16::from_be_bytes([ins.payload[0], ins.payload[1]]);
                println!("{:04}: PushInput {}", i, idx);
            }
            Opcode::CmpI64 => {
                let m = ["EQ","NE","LT","LE","GT","GE"];
                let k = ins.payload.get(0).map(|b| *b as usize).unwrap_or(0);
                println!("{:04}: CmpI64 {}", i, m.get(k).unwrap_or(&"?"));
            }
            Opcode::JsonGetKey => {
                let key = String::from_utf8_lossy(ins.payload);
                println!("{:04}: JsonGetKey "{}"", i, key);
            }
            Opcode::ConstBytes => {
                let s = String::from_utf8_lossy(ins.payload);
                println!("{:04}: ConstBytes "{}"", i, s);
            }
            _ => println!("{:04}: {:?}", i, ins.op),
        }
    }
    Ok(())
}
