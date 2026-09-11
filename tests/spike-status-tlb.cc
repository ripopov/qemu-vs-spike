// Regression checks for data-permission changes with populated Spike TLBs.
#include "processor.h"
#include "mmu.h"
#include "simif.h"
#include <iostream>
#include <stdexcept>
#include <vector>

struct memory_sim : simif_t {
  static constexpr reg_t base = 0x80000000;
  cfg_t cfg;
  std::vector<uint64_t> mem = std::vector<uint64_t>(65536 / 8);
  std::map<size_t, processor_t*> harts;
  memory_sim() { cfg.pmpregions = 0; }
  char* addr_to_mem(reg_t a) override {
    return a >= base && a - base < mem.size()*8 ? (char*)mem.data() + (a-base) : nullptr;
  }
  bool mmio_load(reg_t, size_t, uint8_t*) override { return false; }
  bool mmio_store(reg_t, size_t, const uint8_t*) override { return false; }
  void proc_reset(unsigned) override {}
  const cfg_t& get_cfg() const override { return cfg; }
  const std::map<size_t, processor_t*>& get_harts() const override { return harts; }
  const char* get_symbol(uint64_t) override { return nullptr; }
  void put(reg_t offset, uint64_t v) { mem.at(offset/8) = v; }
};

static void check(bool ok, const char* message) {
  if (!ok) throw std::runtime_error(message);
}
template<class F> static void fault(F f, reg_t cause) {
  try { f(); } catch (trap_t& t) {
    check(t.cause() == cause, "wrong exception cause");
    return;
  }
  throw std::runtime_error("expected access to fault after permission change");
}

static void run(bool hypervisor, bool virt) {
  memory_sim sim;
  processor_t p(hypervisor ? "rv64imafdch_zicsr_zifencei" : "rv64imafdc_zicsr_zifencei",
                "MSU", &sim.cfg, &sim, 0, false, stderr, std::cerr);
  sim.harts[0] = &p;
  p.set_max_vaddr_bits(39);
  p.reset();
  auto* s = p.get_state();
  auto* mmu = p.get_mmu();
  auto pte = [](reg_t offset, reg_t flags) {
    return ((memory_sim::base + offset) >> 12 << 10) | flags;
  };
  sim.put(0x1000, pte(0x2000, PTE_V));
  sim.put(0x2000, pte(0x3000, PTE_V));
  sim.put(0x3000 + 4*8, pte(0x4000, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  sim.put(0x3000 + 5*8, pte(0x5000, PTE_V|PTE_X|PTE_A));
  sim.put(0x3000 + 6*8, pte(0x6000, PTE_V|PTE_R|PTE_X|PTE_A));
  sim.put(0x4000, 42);
  sim.put(0x5000, 0x00000013);
  sim.put(0x6000, 0x00000013);
  p.set_privilege(PRV_S, virt);
  s->satp->write((reg_t(8)<<60) | ((memory_sim::base+0x1000)>>12));
  auto status = [&](reg_t bits) { s->sstatus->write(bits); };
  auto fetch = [&](reg_t addr) { return mmu->access_icache(addr)->data.insn.bits(); };

  for (int i=0; i<8; ++i) {
    status(MSTATUS_SUM);
    check(mmu->load<uint64_t>(0x4000) == 42, "SUM load failed");
    mmu->store<uint64_t>(0x4000, 42);
    fault([&]{ fetch(0x4000); }, CAUSE_FETCH_PAGE_FAULT);
    check(fetch(0x6000) == 0x13, "supervisor fetch failed");
    status(0);
    fault([&]{ mmu->load<uint64_t>(0x4000); }, CAUSE_LOAD_PAGE_FAULT);
    fault([&]{ mmu->store<uint64_t>(0x4000, 9); }, CAUSE_STORE_PAGE_FAULT);
    fault([&]{ mmu->load<uint32_t>(0x5000); }, CAUSE_LOAD_PAGE_FAULT);
    status(MSTATUS_MXR);
    check(mmu->load<uint32_t>(0x5000) == 0x13, "MXR load failed");
    check(fetch(0x5000) == 0x13, "execute-only fetch failed");
    status(0);
    fault([&]{ mmu->load<uint32_t>(0x5000); }, CAUSE_LOAD_PAGE_FAULT);
    check(fetch(0x5000) == 0x13, "MXR changed instruction permission");
  }

  if (virt) {
    // HS MXR also widens VS data permissions, independently of VS MXR.
    reg_t hs = s->mstatus->read();
    s->mstatus->write(hs | MSTATUS_MXR);
    check(mmu->load<uint32_t>(0x5000) == 0x13, "HS MXR load failed");
    check(fetch(0x5000) == 0x13, "HS MXR fetch failed");
    s->mstatus->write(hs & ~MSTATUS_MXR);
    fault([&]{ mmu->load<uint32_t>(0x5000); }, CAUSE_LOAD_PAGE_FAULT);
    check(fetch(0x5000) == 0x13, "HS MXR changed instruction permission");
  }

  // A real instruction-cache invalidation must still expose modified code.
  sim.put(0x6000, 0x00100093); // addi x1, x0, 1
  mmu->flush_icache(); // fence.i uses this same entry point
  check(fetch(0x6000) == 0x00100093, "instruction-cache invalidation failed");
  // A translation invalidation must expose a remapped instruction page.
  sim.put(0x3000 + 6*8, pte(0x5000, PTE_V|PTE_R|PTE_X|PTE_A));
  mmu->flush_tlb();
  check(fetch(0x6000) == 0x13, "full TLB invalidation failed");

  if (!virt) {
    p.set_privilege(PRV_M, false);
    reg_t st = (s->mstatus->read() & ~(MSTATUS_MPP|MSTATUS_MPRV|MSTATUS_SUM|MSTATUS_MXR)) |
               set_field(reg_t(0), MSTATUS_MPP, PRV_S) | MSTATUS_MPRV;
    s->mstatus->write(st);
    check(mmu->load<uint32_t>(0x6000) == 0x13, "MPRV translation failed");
    check(fetch(memory_sim::base+0x6000) == 0x00100093, "MPRV affected instruction fetch");
    s->mstatus->write(st & ~MSTATUS_MPRV);
    fault([&]{ mmu->load<uint32_t>(0x6000); }, CAUSE_LOAD_ACCESS);
    s->mstatus->write(st);
    check(mmu->load<uint32_t>(0x6000) == 0x13, "MPRV reenable failed");
    s->mstatus->write((st & ~MSTATUS_MPP) | MSTATUS_MPP);
    fault([&]{ mmu->load<uint32_t>(0x6000); }, CAUSE_LOAD_ACCESS);
  }
  std::cout << "PASS H=" << hypervisor << " V=" << virt << '\n';
}

int main() {
  try { run(false, false); run(true, false); run(true, true); }
  catch (const std::exception& e) { std::cerr << e.what() << '\n'; return 1; }
  catch (trap_t& t) { std::cerr << "unexpected trap " << t.cause() << " at " << std::hex << t.get_tval() << '\n'; return 1; }
}
