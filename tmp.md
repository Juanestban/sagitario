git tag v1.0.4 ;; git push origin main v1.0.4 ;; git switch chore/horizon-v2.0.0 ;; git merge main
























hay un error en el ci de release en el test de macos "
  ---- suites::suites
  2 of 310 .sag test(s) failed:

  /Users/runner/work/sagitario/sagitario/tests/run-pass/file_metadata.sag: the program: expected exit code 0, got 1
  stdout:
  11 true false false false | 11
  false true | true
  times true true true
  readonly true

  stderr:
  =================================================================
  ==9228==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x625000002100 at pc 0x000108c1b757 bp 0x7ff7b78cb4f0 sp 0x7ff7b78cacb8
  WRITE of size 33990 at 0x625000002100 thread T0
      #0 0x000108c1b756 in readdir+0x7d6 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0xf9756)
      #1 0x00010868410c in sag_ffi_31+0x14 (file_metadata:x86_64+0x10005510c)
      #2 0x000108661ccf in sag_fn_927+0x8d7 (file_metadata:x86_64+0x100032ccf)
      #3 0x00010865ec0a in sag_fn_925+0x4c2 (file_metadata:x86_64+0x10002fc0a)
      #4 0x00010865d1ef in sag_fn_923+0xb77 (file_metadata:x86_64+0x10002e1ef)
      #5 0x000108640c3f in sag_fn_867+0xaa7 (file_metadata:x86_64+0x100011c3f)
      #6 0x000108635c1e in sag_fn_0+0x56e6 (file_metadata:x86_64+0x100006c1e)
      #7 0x0001086300d5 in main+0x13d (file_metadata:x86_64+0x1000010d5)
      #8 0x7ff8172b852f in start+0xbef (dyld:x86_64+0xfffffffffffde52f)

  0x625000002100 is located 0 bytes after 8192-byte regio100)
  allocated by thread T0 here:
      #0 0x000108bbfb22 in malloc+0x82 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0x9db22)
      #1 0x7ff817482b18 in _malloc_type_malloc_outlined+086_64+0x1cb18)
      #2 0x7ff81757924d in _readdir_unlocked+0x75 (libsystem_c.dylib:x86_64+0x7d24d)
      #3 0x7ff817579335 in readdir+0x22 (libsystem_c.dylib:x86_64+0x7d335)
      #4 0x000108c1afa6 in readdir+0x26 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0xf8fa6)
      #5 0x00010868410c in sag_ffi_31+0x14 (file_metadata:x86_64+0x10005510c)
      #6 0x000108661ccf in sag_fn_927+0x8d7 (file_metadata:x86_64+0x100032ccf)
      #7 0x00010865ec0a in sag_fn_925+0x4c2 (file_metadata:x86_64+0x10002fc0a)
      #8 0x00010865d1ef in sag_fn_923+0xb77 (file_metadata:x86_64+0x10002e1ef)
      #9 0x000108640c3f in sag_fn_867+0xaa7 (file_metadat
      #10 0x000108635c1e in sag_fn_0+0x56e6 (file_metadata:x86_64+0x100006c1e)
      #11 0x0001086300d5 in main+0x13d (file_metadata:x86_64+0x1000010d5)
      #12 0x7ff8172b852f in start+0xbef (dyld:x86_64+0xff

  SUMMARY: AddressSanitizer: heap-buffer-overflow (file_metadata:x86_64+0x10005510c) in sag_ffi_31+0x14
  Shadow bytes around the buggy address:
    0x625000001e80: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000001f00: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000001f80: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000002000: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000002080: 00 00 00 00 00 00 00 00 00 00 00 00 0
> hay un error en el ci de release en el test de macos "
  ---- suites::suites
  2 of 310 .sag test(s) failed:

  /Users/runner/work/sagitario/sagitario/tests/run-pass/file_metadata.sag: the program: expected exit code 0, got 1
  stdout:
  11 true false false false | 11
  false true | true
  times true true true
  readonly true

  stderr:
  =================================================================
  ==9228==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x625000002100 at pc 0x000108c1b757 bp 0x7ff7b78cb4f0 sp 0x7ff7b78cacb8
  WRITE of size 33990 at 0x625000002100 thread T0
      #0 0x000108c1b756 in readdir+0x7d6 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0xf9756)
      #1 0x00010868410c in sag_ffi_31+0x14 (file_metadata:x86_64+0x10005510c)
      #2 0x000108661ccf in sag_fn_927+0x8d7 (file_metadata:x86_64+0x100032ccf)
      #3 0x00010865ec0a in sag_fn_925+0x4c2 (file_metadata:x86_64+0x10002fc0a)
      #4 0x00010865d1ef in sag_fn_923+0xb77 (file_metadata:x86_64+0x10002e1ef)
      #5 0x000108640c3f in sag_fn_867+0xaa7 (file_metadata:x86_64+0x100011c3f)
      #6 0x000108635c1e in sag_fn_0+0x56e6 (file_metadata:x86_64+0x100006c1e)
      #7 0x0001086300d5 in main+0x13d (file_metadata:x86_64+0x1000010d5)
      #8 0x7ff8172b852f in start+0xbef (dyld:x86_64+0xfffffffffffde52f)

  0x625000002100 is located 0 bytes after 8192-byte region [0x625000000100,0x625000002100)
  allocated by thread T0 here:
      #0 0x000108bbfb22 in malloc+0x82 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0x9db22)
      #1 0x7ff817482b18 in _malloc_type_malloc_outlined+0x3d (libsystem_malloc.dylib:x86_64+0x1cb18)
      #2 0x7ff81757924d in _readdir_unlocked+0x75 (libsys
      #3 0x7ff817579335 in readdir+0x22 (libsystem_c.dylib:x86_64+0x7d335)
      #4 0x000108c1afa6 in readdir+0x26 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0xf8fa6)
      #5 0x00010868410c in sag_ffi_31+0x14 (file_metadata:x86_64+0x10005510c)
      #6 0x000108661ccf in sag_fn_927+0x8d7 (file_metadata:x86_64+0x100032ccf)
      #7 0x00010865ec0a in sag_fn_925+0x4c2 (file_metadata:x86_64+0x10002fc0a)
      #8 0x00010865d1ef in sag_fn_923+0xb77 (file_metadata:x86_64+0x10002e1ef)
      #9 0x000108640c3f in sag_fn_867+0xaa7 (file_metadata:x86_64+0x100011c3f)
      #10 0x000108635c1e in sag_fn_0+0x56e6 (file_metadat
      #11 0x0001086300d5 in main+0x13d (file_metadata:x86
      #12 0x7ff8172b852f in start+0xbef (dyld:x86_64+0xfffffffffffde52f)

  SUMMARY: AddressSanitizer: heap-buffer-overflow (file_metadata:x86_64+0x10005510c) in sag_ffi_31+0x14
  Shadow bytes around the buggy address:
625000001f00: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000001f80: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000002000: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000002080: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  =>0x625000002100:[fa]fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000002180: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000002200: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000002280: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000002300: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000002380: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
  Shadow byte legend (one shadow byte represents 8 application bytes):
    Addressable:           00
    Partially addressable: 01 02 03 04 05 06 07
    Heap left redzone:       fa
    Freed heap region:       fd
    Stack left redzone:      f1
    Stack mid redzone:       f2
    Stack right redzone:     f3
    Stack after return:      f5
    Stack use after scope:   f8
    Global redzone:          f9
    Global init order:       f6
    Poisoned by user:        f7
    Container overflow:      fc
    Array cookie:            ac
    Intra object redzone:    bb
    ASan internal:           fe
    Left alloca redzone:     ca
    Right alloca redzone:    cb
  ==9228==ABORTING


  /Users/runner/work/sagitario/sagitario/tests/run-pass/std_fs.sag: the program: expected exit code 0, got 1
  stdout:
  before: false false false
  after: true true false
  read: [línea 1<CR><LF>línea 2<LF>más]
  copy: [línea 1<CR><LF>línea 2<LF>más]

  stderr:
  warning[W0005]: unused import `ErrorKind`
    --> /Users/runner/work/sagitario/sagitario/tests/run-
     |
  22 | use std::io::ErrorKind;
     |              ^^^^^^^^^
     = help: remove it

  warning: `/Users/runner/work/sagitario/sagitario/tests/run-pass/std_fs.sag` generated 1 warning
  =================================================================
  ==11118==ERROR: AddressSanitizer: heap-buffer-overflow pc 0x00010e6be757 bp 0x7ff7b1e1af10 sp 0x7ff7b1e1a6d8
  WRITE of size 34865 at 0x625000009900 thread T0
      #0 0x00010e6be756 in readdir+0x7d6 (libclang_rt.asaxf9756)
      #1 0x00010e12a24c in sag_ffi_34+0x14 (std_fs:x86_64+0x10004a24c)
      #2 0x00010e107cef in sag_fn_899+0x8d7 (std_fs:x86_64+0x100027cef)
      #3 0x00010e0f0dca in sag_fn_860+0x4c2 (std_fs:x86_64+0x100010dca)
      #4 0x00010e0e584d in sag_fn_4+0x4315 (std_fs:x86_64
5000009900 is located 0 bytes after 8192-byte region [0x625000007900,0x625000009900)
  allocated by thread T0 here:
      #0 0x00010e662b22 in malloc+0x82 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0x9db22)
      #1 0x7ff817482b18 in _malloc_type_malloc_outlined+0x3d (libsystem_malloc.dylib:x86_64+0x1cb18)
      #2 0x7ff81757924d in _readdir_unlocked+0x75 (libsystem_c.dylib:x86_64+0x7d24d)
      #3 0x7ff817579335 in readdir+0x22 (libsystem_c.dylib:x86_64+0x7d335)
      #4 0x00010e6bdfa6 in readdir+0x26 (libclang_rt.asan_osx_dynamic.dylib:x86_64h+0xf8fa6)
      #5 0x00010e12a24c in sag_ffi_34+0x14 (std_fs:x86_64+0x10004a24c)
      #6 0x00010e107cef in sag_fn_899+0x8d7 (std_fs:x86_64+0x100027cef)
      #7 0x00010e0f0dca in sag_fn_860+0x4c2 (std_fs:x86_64+0x100010dca)
      #8 0x00010e0e584d in sag_fn_4+0x4315 (std_fs:x86_64+0x10000584d)
      #9 0x00010e0e10d5 in main+0x13d (std_fs:x86_64+0x1000010d5)
      #10 0x7ff8172b852f in start+0xbef (dyld:x86_64+0xff

  SUMMARY: AddressSanitizer: heap-buffer-overflow (std_fs:x86_64+0x10004a24c) in sag_ffi_34+0x14
  Shadow bytes around the buggy address:
    0x625000009680: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000009700: 00 00 00 00 00 00 00 00 00 00 00 00 0
625000009800: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
    0x625000009880: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  =>0x625000009900:[fa]fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000009980: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000009a00: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000009a80: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000009b00: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
    0x625000009b80: fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa fa
  Shadow byte legend (one shadow byte represents 8 application bytes):
    Addressable:           00
    Partially addressable: 01 02 03 04 05 06 07
    Heap left redzone:       fa
    Freed heap region:       fd
    Stack left redzone:      f1
    Stack mid redzone:       f2
    Stack right redzone:     f3
    Stack after return:      f5
    Stack use after scope:   f8
    Global redzone:          f9
    Global init order:       f6
    Poisoned by user:        f7
    Container overflow:      fc
    Array cookie:            ac
    Intra object redzone:    bb
    ASan internal:           fe
    Left alloca redzone:     ca
    Right alloca redzone:    cb
  ==11118==ABORTING



  harness: 58 passed, 1 failed
  Error: Process completed with exit code 1.
  "