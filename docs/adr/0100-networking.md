# 0100. Networking: addresses, TCP and UDP sockets (S2)

- Status: Accepted
- Date: 2026-11-13
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 10 of the
  audit of S2), [ADR-0096](0096-streams-files-and-drop.md) (`Read`, `Write` and `Drop`, which
  sockets implement; `io::ErrorKind`), [ADR-0094](0094-conversions-and-defaults.md) (interfaces
  the compiler implements for types it owns); uses
  [ADR-0099](0099-associated-and-composite-constants.md)

## Context

Record 10 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is Rust's `std::net`: IP and
socket addresses, TCP listeners and streams, and UDP sockets, on the systems' sockets in
`std::sys` and on the `Read` and `Write` interfaces of record 7. Sagitario has none of it; a
program reaches sockets through `extern "C"` only. TLS and HTTP are Nova packages, as in Rust.

The record follows Rust, with these differences of the language:

- Rust's `ToSocketAddrs` is implemented for `&str`, `(&str, u16)`, `(IpAddr, u16)` and the
  addresses. A block `impl ... for` goes in the module of its type (`E0343`), so `std::net`
  cannot implement it for `String` or tuples; the compiler does, as it implements `Into`
  (ADR-0094, Q633).
- A class whose fields are `var` is not `Send` (ADR-0045), and a server runs a thread for each
  connection, so the sockets are `Send` by name, as the classes of `std::sync` and `Thread`
  ([ADR-0098](0098-locks-orderings-and-threads.md)). The system orders the operations on one
  socket.
- Rust's `parse::<SocketAddr>()` is `FromStr`; Sagitario's conversion from text is
  `TryFrom<String, E>` (ADR-0094, Q634).
- The debug form of a struct is the compiler's (spec 02): `{ip:?}` is
  `Ipv4Addr { bytes: [127, 0, 0, 1] }`, and `{ip}`, its `Display`, is Rust's `127.0.0.1`.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q690 | IP addresses | `Ipv4Addr`, a struct: `Ipv4Addr::new(a, b, c, d)`, `octets() -> [u8; 4]`, `to_bits() -> u32` and `from_bits(bits)`, the constants `LOCALHOST`, `UNSPECIFIED` and `BROADCAST` (ADR-0099), `is_loopback`, `is_unspecified`, `is_private`, `is_link_local`, `is_multicast`, `is_broadcast`, `is_documentation`, `to_ipv6_mapped()` and `to_ipv6_compatible()`. `Ipv6Addr`: `new(a, b, c, d, e, f, g, h)` of `u16` segments, `segments() -> [u16; 8]`, `octets() -> [u8; 16]`, `LOCALHOST` and `UNSPECIFIED`, `is_loopback`, `is_unspecified`, `is_multicast`, `is_unique_local`, `is_unicast_link_local`, `to_ipv4_mapped() -> Option<Ipv4Addr>` and `to_ipv4()`. `IpAddr`, an enum of `V4(Ipv4Addr)` and `V6(Ipv6Addr)`: `is_ipv4`, `is_ipv6`, `is_loopback`, `is_unspecified`, `is_multicast`, and `From` each of the two. All as Rust's. |
| Q691 | Socket addresses | `SocketAddrV4`: `new(ip, port)`, `ip()`, `port()`, `set_ip`, `set_port`. `SocketAddrV6`: `new(ip, port, flowinfo, scope_id)`, with those getters and setters. `SocketAddr`, an enum of `V4(SocketAddrV4)` and `V6(SocketAddrV6)`: `new(ip: IpAddr, port)`, `ip()`, `port()`, `set_ip`, `set_port`, `is_ipv4`, `is_ipv6`, and `From` each of the two. All as Rust's. |
| Q692 | Text, comparisons | Every address implements `Display` with Rust's text: `127.0.0.1`, IPv6 as RFC 5952 writes it (`::1`, `::ffff:192.0.2.1` for a mapped address), `127.0.0.1:80`, `[::1]:80` and `[fe80::1%2]:80` with a scope. Each implements `TryFrom<String, AddrParseError>`, which accepts Rust's forms: `Ipv4Addr::try_from("10.0.0.1")`, `const a: SocketAddr = text.try_into()?`. `AddrParseError` implements `Error`, with Rust's messages (`invalid IPv4 address syntax`, `invalid socket address syntax`...). Every address implements `Hash` and `Ord`, as Rust's: an `IpAddr::V4` before any `V6`. |
| Q693 | `ToSocketAddrs` | `interface ToSocketAddrs { fn to_socket_addrs(self) -> Result<Iter<SocketAddr>, io::Error>; }`, which `std::net` implements for `SocketAddr`, `SocketAddrV4` and `SocketAddrV6`, and the compiler for `String` (`"host:port"`: an address as is, otherwise the name looked up with the system's resolver, `getaddrinfo`), `(String, u16)`, `(IpAddr, u16)`, `(Ipv4Addr, u16)` and `(Ipv6Addr, u16)`. The functions taking an address are generic, `bind<A: ToSocketAddrs>(addr: A)`, and try each address in order, as Rust's: the first that works, else the last error, or `InvalidInput` (`could not resolve to any addresses`) when there is none. A text without a port is `InvalidInput` (`invalid socket address`); a name the resolver does not know is an error with its message. |
| Q694 | TCP | `TcpListener::bind(addr) -> Result<TcpListener, io::Error>`, `accept() -> Result<(TcpStream, SocketAddr), io::Error>`, `incoming() -> Iter<Result<TcpStream, io::Error>>`, which never ends, and `local_addr()`. `TcpStream::connect(addr) -> Result<TcpStream, io::Error>`, implementing `Read` and `Write` (whose `flush` does nothing), `peer_addr()`, `local_addr()`, and `shutdown(how: Shutdown)` with `Shutdown::Read`, `Write` and `Both`. Each socket implements `Drop`, which closes it, as `File`; and has `as_raw_handle()` and `unsafe from_raw_handle(handle)`, as `File`. A write to a peer that closed is an error of kind `BrokenPipe`, never the `SIGPIPE` signal that would end the program: the writes ask the system not to send it (`MSG_NOSIGNAL`, or `SO_NOSIGPIPE` on Apple's systems), as Rust's do. |
| Q695 | UDP | `UdpSocket::bind(addr)`, `send_to(bytes, addr) -> Result<usize, io::Error>`, `recv_from(buffer: &mut [u8]) -> Result<(usize, SocketAddr), io::Error>`, `connect(addr)`, then `send(bytes)` and `recv(buffer)`, `peer_addr()`, `local_addr()`, and the raw handles of Q694. A datagram longer than the buffer is cut to it, as Rust's says. |
| Q696 | Options | On `TcpStream` and `UdpSocket`: `set_read_timeout(Option<Duration>)` and `set_write_timeout`, with `read_timeout()` and `write_timeout()`, a zero duration being `InvalidInput` (`cannot set a 0 duration timeout`) and a time out an error of kind `WouldBlock` or `TimedOut`, as the system gives it; `set_ttl(u32)` and `ttl()`; `set_nonblocking(bool)`, after which an operation that would wait is a `WouldBlock` error; `take_error() -> Result<Option<io::Error>, io::Error>`; `try_clone() -> Result<Self, io::Error>`. On `TcpStream`: `connect_timeout(addr: SocketAddr, timeout)`, `set_nodelay(bool)` and `nodelay()`, and `peek(buffer)`. On `UdpSocket`: `peek_from(buffer)`, `peek(buffer)`, `set_broadcast(bool)` and `broadcast()`. On `TcpListener`: `set_ttl`, `ttl`, `set_nonblocking`, `take_error` and `try_clone`. All as Rust's. |
| Q697 | Kinds of errors | `io::ErrorKind` gains Rust's `ConnectionRefused`, `ConnectionReset`, `ConnectionAborted`, `NotConnected`, `AddrInUse`, `AddrNotAvailable`, `BrokenPipe`, `WouldBlock`, `TimedOut` and `Interrupted`, from the `errno` values of each system and the `WSA` codes of Windows. Files, processes and the rest of `std` give them too, where the system does. A `match` on `ErrorKind` without `_` must add them: this changes existing programs. |
| Q698 | The systems | POSIX sockets and Winsock 2 in `std::sys`. Every socket is created so that child processes do not inherit it (`SOCK_CLOEXEC`, `FD_CLOEXEC` on Apple's systems, `WSA_FLAG_NO_HANDLE_INHERIT`), as Rust's. Winsock is started (`WSAStartup`) before the first socket, and never stopped, as Rust's. The driver links `ws2_32` on Windows for a program that uses `std::net`. Operations interrupted by a signal (`EINTR`) are tried again, except where Rust gives `Interrupted`. |
| Q699 | Out of the record | Multicast (`join_multicast_v4` and the rest), `set_only_v6`, `set_linger`, the sockets of Unix (`std::os::unix::net`), and sockets for `async` tasks: blocking sockets and threads only. |
| Q700 | The editor | `sag lsp` completes the types, functions and constants of `std::net` as any of `std`; the README lists the module. |
| Q701 | Increments | **1** The addresses (Q690–Q692). **2** TCP: `ToSocketAddrs` and the compiler's implementations, `TcpListener` and `TcpStream`, Winsock and `ws2_32`, and the new kinds of errors (Q693, Q694, Q697, Q698). **3** UDP (Q695). **4** The options (Q696). Each one with its tests, the spec, the editor and the rows of the audit. The tests talk to themselves on the loopback addresses with port 0, which the system replaces by a free port; an IPv6 test passes on a system without IPv6, where binding `::1` fails with `AddrNotAvailable`. |

### Example

```sag
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;

fn main() {
  const listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
  const address = listener.local_addr().unwrap();
  const server = thread::spawn(() => {
    const (stream, _) = listener.accept().unwrap();
    const buffer = new List<u8>();

    buffer.resize(5, 0);
    stream.read_exact(&mut buffer).unwrap();
    stream.write_all(&buffer).unwrap();
  });

  const client = TcpStream::connect(address).unwrap();

  client.write_str("hello").unwrap();
  println(f"{client.read_to_string().unwrap()} from {address}");   // hello from 127.0.0.1:...
  server.join();
}
```

## Alternatives considered

- **Addresses as `String` only** (`connect("127.0.0.1:80")`): no work in the compiler, but Rust's
  code with tuples and `SocketAddr` would not read the same.
- **`connect` and `connect_addr`**, one for text and one for `SocketAddr`: no work in the compiler,
  but each function twice, under names Rust does not have.
- **Sockets that are not `Send`:** each socket would stay in the thread that made it, and a
  server would answer one connection at a time.
- **Network errors as `ErrorKind::Other`:** no change to existing programs, but a program could
  not tell a time out from a refused connection without reading the message.
- **Multicast now:** it fits the same sockets, but no program of the harness needs it yet.

## Consequences

- Programs serve and connect over TCP and UDP without `extern "C"`, with Rust's API.
- `io::ErrorKind` changes, which existing programs matching it without `_` must follow.
- The compiler implements `ToSocketAddrs` for `String` and four tuples, and knows the sockets as
  `Send` by name.
- The audit's row of `net` is done, and with it the records of S2.

## Notes

- Increment 1 is done, in `std::net`, which the loader and `sag lsp` know as any module of
  `std`. The parser follows Rust's (`std::net::parser`): each form is tried at a position, which
  goes back if it is not there, an IPv4 octet takes no zero before a digit, and an IPv4 address
  may end an IPv6 one where two segments are left. `AddrParseError` takes a `std::sys::Message`,
  which programs cannot make, as the results of `std::sync` take a `std::sys::Flag`. A text is
  converted with a typed result, `const a: Result<SocketAddr, AddrParseError> = text.try_into();`:
  `try_into()?` alone does not tell the compiler the type to convert to.
- Increment 2 is done. `std::sys` writes the `sockaddr_in` and `sockaddr_in6` of each system byte
  by byte (Apple's start with their length), and reads `getaddrinfo`'s list, whose fields each C
  library orders its own way. The compiler knows `ToSocketAddrs`, `IpAddr`, `Ipv4Addr` and
  `Ipv6Addr` of `std::net` by their ids: `implements` gives the interface to `String` and the five
  pairs, a call of `to_socket_addrs` on them lowers to a private function of `std::net` for each
  (`text_socket_addrs` and the rest), and a pair written for a type parameter bounded by
  `ToSocketAddrs` takes `u16` for its port, so that `bind((ip, 0))` is Rust's. `TcpListener` and
  `TcpStream` join the classes that are `Send` by name. The `errno` values of the network differ
  between C libraries, so `std::sys` has them for each; Windows' C runtime describes them, and
  `std::sys` maps the `WSA` codes to them, keeping the others, which `FormatMessageW` describes.
  Where Rust gives `Interrupted` from a read or a write, the call is tried again, since the
  defaults of `Read` and `Write` do not try again as Rust's do. On Apple's systems a socket gets
  no `FD_CLOEXEC`, which takes the variadic `fcntl`: `std::process` closes in its children every
  descriptor it does not give them. The driver links `ws2_32` on Windows when the program calls
  `WSAStartup`. `for` over an iterator that is a place, such as `option.unwrap()`, was an internal
  compiler error, found by the tests, and fixed.
- Increment 3 is done. `std::sys` gains `sendto` and `recvfrom` for each system. Windows reports
  a datagram longer than the buffer as the error `WSAEMSGSIZE`, with the buffer filled; `std::sys`
  gives the bytes the buffer holds instead, as POSIX systems do, which is what Rust's says. As
  Rust's, `send_to` sends to the first address of what it is given, and `connect` tries each.
  `UdpSocket` joins the classes that are `Send` by name.

