import sys, struct, json

OPCODES = {
    0x01: "ConstI64",
    0x02: "ConstBytes",
    0x03: "JsonNormalize",
    0x04: "JsonValidate",
    0x05: "AddI64",
    0x06: "SubI64",
    0x07: "MulI64",
    0x08: "CmpI64",
    0x09: "AssertTrue",
    0x0A: "HashBlake3",
    0x0B: "CasPut",
    0x0C: "CasGet",
    0x0D: "SetRcBody",
    0x0E: "AttachProof",
    0x0F: "SignDefault",
    0x10: "EmitRc",
    0x11: "Drop",
    0x12: "PushInput",
    0x13: "JsonGetKey",
}

CMP_OPS = {0: "EQ", 1: "NE", 2: "LT", 3:"LE", 4:"GT", 5:"GE"}

def read_tlv(b, i):
    if i+3 > len(b): 
        raise ValueError("Truncated")
    op = b[i]
    ln = struct.unpack(">H", b[i+1:i+3])[0]
    pl = b[i+3:i+3+ln]
    return op, pl, i+3+ln

def main(path):
    b = open(path, "rb").read()
    i = 0
    n = 0
    while i < len(b):
        op, pl, i = read_tlv(b, i)
        name = OPCODES.get(op, f"OP_0x{op:02X}")
        if name == "ConstI64":
            v = struct.unpack(">q", pl)[0]
            print(f"{n:04d}: {name} {v}")
        elif name == "ConstBytes":
            show = pl.decode("utf-8", "ignore")
            # Truncate long payloads
            if len(show) > 80: show = show[:77] + "..."
            print(f'{n:04d}: {name} "{show}"')
        elif name == "PushInput":
            idx = struct.unpack(">H", pl)[0]
            print(f"{n:04d}: {name} {idx}")
        elif name == "CmpI64":
            opi = pl[0] if pl else 0
            print(f"{n:04d}: {name} {CMP_OPS.get(opi, opi)}")
        elif name == "JsonGetKey":
            key = pl.decode("utf-8")
            print(f'{n:04d}: {name} "{key}"')
        else:
            print(f"{n:04d}: {name}")
        n += 1

if __name__ == "__main__":
    main(sys.argv[1])
