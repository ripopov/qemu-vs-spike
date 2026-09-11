// Regression checks for host console polling: input is still delivered, a
// burst is drained on consecutive calls, polling is rate limited between
// bursts, and it stops after end of input.
#include "term.h"
#include <chrono>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <unistd.h>

static void check(bool ok, const char* message) {
  if (!ok) throw std::runtime_error(message);
}

int main() {
  try {
    int fds[2];
    check(pipe(fds) == 0, "pipe");
    check(dup2(fds[0], 0) == 0, "dup2");
    check(write(fds[1], "ab", 2) == 2, "write");
    std::string got;
    int calls = 0;
    while (got.size() < 2 && calls < 10) {
      int ch = canonical_terminal_t::read();
      calls++;
      if (ch != -1) got.push_back((char)ch);
    }
    check(got == "ab", "burst was not delivered");
    check(calls == 2, "burst was not drained on consecutive calls");
    // The call after a burst still polls (and finds nothing); from then on
    // input arriving between polls is delivered at the next poll, within the
    // documented interval, and the calls in between do not poll the host.
    check(canonical_terminal_t::read() == -1, "spurious input");
    check(write(fds[1], "c", 1) == 1, "write");
    auto start = std::chrono::steady_clock::now();
    int ch = -1;
    calls = 0;
    while (ch == -1) { ch = canonical_terminal_t::read(); calls++; check(calls < 100000000, "late input never delivered"); }
    double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
    check(ch == 'c', "late input was not delivered");
    check(calls > 1, "polling was not rate limited");
    check(ms < 100, "late input took too long");
    // After end of input nothing is delivered and the host is not polled.
    close(fds[1]);
    for (int i = 0; i < 200000; i++)
      check(canonical_terminal_t::read() == -1, "input after end of input");
    std::printf("PASS console polling (%d calls, %.2f ms until late input)\n", calls, ms);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
  }
}
