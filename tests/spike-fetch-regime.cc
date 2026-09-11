// Regression checks for instruction caches kept across privilege changes.
// Each regime (privilege, virtualization) must only use translations and
// decoded instructions that were validated for it, and every existing
// invalidation (fence.i, sfence.vma/satp, Debug Mode) must still reach the
// caches of regimes that are not currently selected.
#include "processor.h"
#include "mmu.h"
#include "simif.h"
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <vector>

struct memory_sim : simif_t {
  static constexpr reg_t base = 0x80000000;
  cfg_t cfg;
  std::vector<uint64_t> mem = std::vector<uint64_t>((4 << 20) / 8);
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
  void put16(reg_t offset, uint16_t v) { memcpy((char*)mem.data() + offset, &v, 2); }
};

static void check(bool ok, const char* message) {
  if (!ok) throw std::runtime_error(message);
}
template<class F> static void fault(F f, reg_t cause, const char* message) {
  try { f(); } catch (trap_t& t) {
    check(t.cause() == cause, message);
    return;
  }
  throw std::runtime_error(message);
}

static const reg_t NOP = 0x00000013, ADDI1 = 0x00100093, ADDI2 = 0x00200093, ADDI3 = 0x00300093;

static void run(bool hypervisor) {
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
  const reg_t user_page = 0x4000, kernel_page = 0x5000, spare_page = 0x6000;
  sim.put(0x1000, pte(0x2000, PTE_V));
  sim.put(0x2000, pte(0x3000, PTE_V));
  sim.put(0x3000 + 4*8, pte(user_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(user_page, NOP);
  sim.put(kernel_page, ADDI1);
  sim.put(spare_page, ADDI2);
  auto fetch = [&](reg_t addr) { return mmu->access_icache(addr)->data.insn.bits(); };
  auto priv = [&](reg_t prv, bool v = false) { p.set_privilege(prv, v); };

  priv(PRV_S);
  s->satp->write((reg_t(8)<<60) | ((memory_sim::base+0x1000)>>12));

  // 1. Regime isolation: entries filled in U-mode are not visible to S-mode.
  priv(PRV_U);
  check(fetch(user_page) == NOP, "user fetch failed");
  fault([&]{ fetch(kernel_page); }, CAUSE_FETCH_PAGE_FAULT, "user fetched a supervisor page");
  priv(PRV_S);
  check(fetch(kernel_page) == ADDI1, "supervisor fetch failed");
  fault([&]{ fetch(user_page); }, CAUSE_FETCH_PAGE_FAULT, "supervisor executed a user page (SUM does not apply to fetch)");
  s->sstatus->write(s->sstatus->read() | MSTATUS_SUM);
  fault([&]{ fetch(user_page); }, CAUSE_FETCH_PAGE_FAULT, "supervisor executed a user page with SUM");
  priv(PRV_U);
  fault([&]{ fetch(kernel_page); }, CAUSE_FETCH_PAGE_FAULT, "user fetched a supervisor page after round trip");

  // 2. Retention: without fence.i, a privilege round trip may keep the decoded
  //    instruction (the architecture permits stale fetches until FENCE.I).
  sim.put(user_page, ADDI3);
  check(fetch(user_page) == NOP, "expected retained decoded instruction");
  priv(PRV_S); priv(PRV_M); priv(PRV_U);
  check(fetch(user_page) == NOP, "privilege round trip should retain the user instruction cache");

  // 3. fence.i issued in another regime must invalidate the user copy.
  priv(PRV_S);
  mmu->flush_icache();
  priv(PRV_U);
  check(fetch(user_page) == ADDI3, "fence.i from supervisor mode did not reach the user instruction cache");

  // 4. Translation invalidation issued in S-mode (sfence.vma/satp write) must
  //    invalidate user instruction translations and decoded instructions.
  priv(PRV_S);
  sim.put(0x3000 + 4*8, pte(spare_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  mmu->flush_tlb();
  priv(PRV_U);
  check(fetch(user_page) == ADDI2, "sfence.vma from supervisor mode did not reach the user regime");
  sim.put(0x3000 + 4*8, pte(user_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  s->satp->write(s->satp->read()); // unchanged satp value: no flush
  check(fetch(user_page) == ADDI2, "unchanged satp must not invalidate");
  s->satp->write(0);
  s->satp->write((reg_t(8)<<60) | ((memory_sim::base+0x1000)>>12));
  check(fetch(user_page) == ADDI3, "satp write did not invalidate the user regime");

  // 5. Revoking execute permission with a fence must be observed by the
  //    regime that cached the page, even after other regimes ran in between.
  priv(PRV_S);
  check(fetch(kernel_page) == ADDI1, "supervisor cache lost");
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_A|PTE_D));
  priv(PRV_U); check(fetch(user_page) == ADDI3, "user fetch after S");
  priv(PRV_S);
  check(fetch(kernel_page) == ADDI1, "stale translation may be used before sfence.vma");
  mmu->flush_tlb();
  fault([&]{ fetch(kernel_page); }, CAUSE_FETCH_PAGE_FAULT, "execute revocation not observed after sfence.vma");
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  mmu->flush_tlb();

  // 6. Machine mode uses physical addresses and its own set.
  priv(PRV_M);
  check(fetch(memory_sim::base + user_page) == ADDI3, "machine fetch failed");
  fault([&]{ fetch(user_page); }, CAUSE_FETCH_ACCESS, "virtual address used in machine mode");
  priv(PRV_U);
  check(fetch(user_page) == ADDI3, "user regime after machine mode");

  // 7. Debug Mode entry and exit discard every cached instruction.
  sim.put(user_page, NOP);
  check(fetch(user_page) == ADDI3, "expected retained instruction before debug entry");
  s->debug_mode = true;
  priv(PRV_M);
  s->debug_mode = false;
  priv(PRV_U);
  check(fetch(user_page) == NOP, "debug mode transition did not invalidate the user regime");

#ifdef TEST_ADDRESS_FENCE
  // 8. SFENCE.VMA with an address invalidates the fenced page in every
  //    regime but keeps translations in other largest-leaf regions.  Map two
  //    pages one gigabyte away through their own tables, in 4 KiB slots that
  //    do not alias the direct-mapped cache slots of the near pages.
  sim.put(0x1000 + 1*8, pte(0x7000, PTE_V));
  sim.put(0x7000, pte(0x8000, PTE_V));
  sim.put(0x8000 + 5*8, pte(0x9000, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  sim.put(0x8000 + 6*8, pte(0xa000, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(0x9000, ADDI1);
  sim.put(0xa000, ADDI1);
  const reg_t far_user = (reg_t(1) << 30) + 0x5000, far_kernel = (reg_t(1) << 30) + 0x6000;
  priv(PRV_S);
  mmu->flush_tlb();
  // Within one regime: data and instruction translations of the fenced page
  // go, those of the other region stay.
  check(fetch(kernel_page) == ADDI1 && fetch(far_kernel) == ADDI1, "two-region fetch setup failed");
  check(mmu->load<uint64_t>(kernel_page) == ADDI1 && mmu->load<uint64_t>(far_kernel) == ADDI1, "two-region load setup failed");
  sim.put(0x3000 + 5*8, pte(spare_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(0x8000 + 6*8, pte(spare_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(spare_page, ADDI3);
  mmu->flush_tlb_vaddr(kernel_page + 0x10); // as sfence.vma with rs1 inside the page
  check(fetch(kernel_page) == ADDI3, "address fence did not invalidate the fenced instruction page");
  check(mmu->load<uint64_t>(kernel_page) == ADDI3, "address fence did not invalidate the fenced data translation");
  check(fetch(far_kernel) == ADDI1, "address fence must not discard instructions in another region");
  check(mmu->load<uint64_t>(far_kernel) == ADDI1, "address fence must not discard data translations in another region");
  mmu->flush_tlb_vaddr(far_kernel);
  check(fetch(far_kernel) == ADDI3 && mmu->load<uint64_t>(far_kernel) == ADDI3, "second address fence failed");
  // Across regimes: the user instruction cache observes a fence issued in
  // supervisor mode for its page and keeps its other region.
  priv(PRV_U);
  check(fetch(user_page) == NOP && fetch(far_user) == ADDI1, "user two-region setup failed");
  sim.put(0x3000 + 4*8, pte(spare_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  sim.put(0x8000 + 5*8, pte(spare_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  priv(PRV_S);
  mmu->flush_tlb_vaddr(user_page + 0x10);
  priv(PRV_U);
  check(fetch(user_page) == ADDI3, "address fence did not reach the user regime");
  check(fetch(far_user) == ADDI1, "address fence discarded another region of the user regime");
  // A fence for any address inside a 2 MiB leaf page discards every
  // translation of that leaf page but not those of a 4 KiB page next to it.
  priv(PRV_S);
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D)); // undo the remap above
  sim.put(0x2000 + 1*8, pte(0, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D)); // VA 0x200000-0x3fffff -> RAM base
  const reg_t big_a = 0x200000 + 0xb000, big_b = 0x200000 + 0xc000;
  sim.put(0xb000, ADDI1);
  sim.put(0xc000, ADDI2);
  mmu->flush_tlb();
  check(fetch(big_a) == ADDI1 && fetch(big_b) == ADDI2, "superpage fetch failed");
  check(mmu->load<uint64_t>(big_a) == ADDI1 && mmu->load<uint64_t>(big_b) == ADDI2, "superpage load failed");
  check(fetch(kernel_page) == ADDI1 && mmu->load<uint64_t>(kernel_page) == ADDI1, "kernel page setup failed");
  sim.put(0xb000, ADDI3);
  sim.put(0xc000, ADDI3);
  sim.put(0x2000 + 1*8, pte(0, PTE_V|PTE_X|PTE_A|PTE_D)); // execute-only: loads must fault
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_X|PTE_A|PTE_D));
  mmu->flush_tlb_vaddr(big_b + 0x100);
  check(fetch(big_a) == ADDI3 && fetch(big_b) == ADDI3, "superpage fence did not discard both pages of the leaf");
  fault([&]{ mmu->load<uint64_t>(big_a); }, CAUSE_LOAD_PAGE_FAULT, "superpage fence did not discard the sibling data translation");
  fault([&]{ mmu->load<uint64_t>(big_b); }, CAUSE_LOAD_PAGE_FAULT, "superpage fence did not discard the fenced data translation");
  check(fetch(kernel_page) == ADDI1, "superpage fence discarded a 4 KiB instruction page");
  check(mmu->load<uint64_t>(kernel_page) == ADDI1, "superpage fence discarded a 4 KiB data translation");
  mmu->flush_tlb_vaddr(kernel_page);
  fault([&]{ mmu->load<uint64_t>(kernel_page); }, CAUSE_LOAD_PAGE_FAULT, "4 KiB fence after superpage fence failed");
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));

  // An instruction straddling the end of the 2 MiB leaf and a 4 KiB page,
  // fetched while translations are not cached (commit logging disables the
  // TLBs), must still be discarded by a fence for another granule of the
  // superpage: its page size is recorded as unknown.
  sim.put(0x2000 + 1*8, pte(0, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(0x2000 + 2*8, pte(0xe000, PTE_V));
  sim.put(0xe000, pte(0x200000, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D)); // VA 0x400000 -> RAM + 2 MiB
  const reg_t straddle = 0x400000 - 2;
  sim.put16(0x1ffffe, 0x0093); sim.put16(0x200000, 0x0010); // addi x1, x0, 1 across the boundary
  p.enable_log_commits();
  mmu->flush_tlb();
  check(fetch(straddle) == ADDI1, "straddling fetch failed");
  sim.put16(0x1ffffe, 0x0113); sim.put16(0x200000, 0x0020); // addi x2, x0, 2
  mmu->flush_tlb_vaddr(0x300000);
  check(fetch(straddle) == 0x00200113, "fence for another granule of the superpage did not discard a straddling instruction fetched without a TLB entry");
  sim.put(0x2000 + 1*8, 0);
  sim.put(0x2000 + 2*8, 0);

  // Restore the original mappings for the remaining checks.
  priv(PRV_S);
  sim.put(0x3000 + 4*8, pte(user_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_U|PTE_A|PTE_D));
  sim.put(0x3000 + 5*8, pte(kernel_page, PTE_V|PTE_R|PTE_W|PTE_X|PTE_A|PTE_D));
  sim.put(user_page, NOP);
  mmu->flush_tlb();
  priv(PRV_U);
  check(fetch(user_page) == NOP, "restore failed");
#endif

  if (hypervisor) {
    // 9. Virtual supervisor mode is a separate regime from HS mode even with a
    //    bare G-stage: its caches are filled from vsatp, not satp.
    priv(PRV_S, true);
    s->vsatp->write(0); // VS bare: guest physical == guest virtual, no G-stage
    check(fetch(memory_sim::base + kernel_page) == ADDI1, "VS bare fetch failed");
    priv(PRV_S, false);
    check(fetch(kernel_page) == ADDI1, "HS fetch after VS failed");
    fault([&]{ fetch(memory_sim::base + kernel_page); }, CAUSE_FETCH_PAGE_FAULT, "HS used a VS translation");
    priv(PRV_S, true);
    sim.put(kernel_page, ADDI2);
    check(fetch(memory_sim::base + kernel_page) == ADDI1, "VS regime not retained");
    mmu->flush_icache();
    check(fetch(memory_sim::base + kernel_page) == ADDI2, "VS fence.i failed");
    priv(PRV_S, false);
    check(fetch(kernel_page) == ADDI2, "HS regime missed fence.i from VS");
  }
  std::cout << "PASS H=" << hypervisor << '\n';
}

int main() {
  try { run(false); run(true); }
  catch (const std::exception& e) { std::cerr << e.what() << '\n'; return 1; }
  catch (trap_t& t) { std::cerr << "unexpected trap " << t.cause() << " at " << std::hex << t.get_tval() << '\n'; return 1; }
}
