// Validation only: reconstruct canonical RV64C expansions using Spike extractors.
#include "decode.h"
#include <cstdint>
#include <cstdio>
#include <optional>

static uint32_t iop(uint32_t op, uint32_t rd, uint32_t f, uint32_t rs, int64_t imm) {
  return ((uint32_t(imm) & 4095) << 20) | (rs << 15) | (f << 12) | (rd << 7) | op;
}
static uint32_t rop(uint32_t rd, uint32_t f, uint32_t a, uint32_t b, uint32_t hi, uint32_t op=0x33) {
  return (hi << 25) | (b << 20) | (a << 15) | (f << 12) | (rd << 7) | op;
}
static uint32_t sop(uint32_t op, uint32_t f, uint32_t a, uint32_t b, uint32_t imm) {
  return ((imm & 0xfe0) << 20) | (b << 20) | (a << 15) | (f << 12) | ((imm & 31) << 7) | op;
}
static std::optional<uint32_t> expand(uint16_t c) {
  insn_t x(c);
  auto rd=x.rvc_rd(), rs=x.rvc_rs2(), a=x.rvc_rs1s(), b=x.rvc_rs2s();
  auto imm=x.rvc_imm(), uimm=x.rvc_zimm();
  switch (c & 0xe003) {
    case 0x0000:
      if (!x.rvc_addi4spn_imm()) return {};
      return iop(0x13,b,0,2,x.rvc_addi4spn_imm());
    case 0x2000: return iop(7,b,3,a,x.rvc_ld_imm());
    case 0x4000: return iop(3,b,2,a,x.rvc_lw_imm());
    case 0x6000: return iop(3,b,3,a,x.rvc_ld_imm());
    case 0xa000: return sop(0x27,3,a,b,x.rvc_ld_imm());
    case 0xc000: return sop(0x23,2,a,b,x.rvc_lw_imm());
    case 0xe000: return sop(0x23,3,a,b,x.rvc_ld_imm());
    case 0x0001: return iop(0x13,rd,0,rd,imm);
    case 0x2001:
      if (!rd) return {};
      return iop(0x1b,rd,0,rd,imm);
    case 0x4001: return iop(0x13,rd,0,0,imm);
    case 0x6001:
      if (rd==2) {
        if (!x.rvc_addi16sp_imm()) return {};
        return iop(0x13,2,0,2,x.rvc_addi16sp_imm());
      }
      if (!imm) return {};
      return (uint32_t(imm)<<12) | (rd<<7) | 0x37;
    case 0x8001:
      switch ((c>>10)&3) {
        case 0: return iop(0x13,a,5,a,uimm);
        case 1: return iop(0x13,a,5,a,uimm | 0x400);
        case 2: return iop(0x13,a,7,a,imm);
        default:
          switch (c & 0x1060) {
            case 0: return rop(a,0,a,b,32);
            case 0x20: return rop(a,4,a,b,0);
            case 0x40: return rop(a,6,a,b,0);
            case 0x60: return rop(a,7,a,b,0);
            case 0x1000: return rop(a,0,a,b,32,0x3b);
            case 0x1020: return rop(a,0,a,b,0,0x3b);
            default: return {};
          }
      }
    case 0xa001: {
      uint32_t n=x.rvc_j_imm();
      return ((n&0x100000)<<11) | ((n&0x7fe)<<20) | ((n&0x800)<<9) | (n&0xff000) | 0x6f;
    }
    case 0xc001: case 0xe001: {
      uint32_t n=x.rvc_b_imm();
      return ((n&0x1000)<<19) | ((n&0x7e0)<<20) | (a<<15) | (((c>>13)&1)<<12) |
             ((n&0x1e)<<7) | ((n&0x800)>>4) | 0x63;
    }
    case 0x0002: return iop(0x13,rd,1,rd,uimm);
    case 0x2002: return iop(7,rd,3,2,x.rvc_ldsp_imm());
    case 0x4002:
      if (!rd) return {};
      return iop(3,rd,2,2,x.rvc_lwsp_imm());
    case 0x6002:
      if (!rd) return {};
      return iop(3,rd,3,2,x.rvc_ldsp_imm());
    case 0x8002:
      if (!(c & 0x1000)) {
        if (rs) return rop(rd,0,0,rs,0);
        if (!rd) return {};
        return iop(0x67,0,0,rd,0);
      }
      if (rs) return rop(rd,0,rd,rs,0);
      if (!rd) return 0x00100073;
      return iop(0x67,1,0,rd,0);
    case 0xa002: return sop(0x27,3,2,rs,x.rvc_sdsp_imm());
    case 0xc002: return sop(0x23,2,2,rs,x.rvc_swsp_imm());
    case 0xe002: return sop(0x23,3,2,rs,x.rvc_sdsp_imm());
    default: return {};
  }
}
int main() {
  for (uint32_t c=0; c<65536; ++c) {
    auto i=expand(c);
    if (i) std::printf("%04x %08x\n",c,*i);
    else std::printf("%04x -\n",c);
  }
}
