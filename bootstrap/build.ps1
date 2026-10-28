# Builds Sagitario from the seed with a C compiler alone (docs/adr/0067-bootstrap-from-the-seed.md):
# the seed is stage 0, which builds stage 1, which builds stage 2; stages 1 and 2 must generate the
# same C. Writes `build\bin\sag.exe` and `build\bin\nova.exe`. Windows; elsewhere, `build.sh`.

$ErrorActionPreference = 'Stop'

$usage = @'
Usage: bootstrap\build.ps1 [--debug] [--install] [--update-seed] [--dist]

  --debug        Build stages 1 and 2 without optimizations; integer overflow panics
  --install      Copy sag and nova to %USERPROFILE%\.sagitario\bin and add it to PATH
  --update-seed  Write the C of stage 2 and the runtime to bootstrap\
  --dist         Write the archive of a release for this system to build\dist\
  --help         Show this message

The C compiler is SAG_CC, else clang. With cl, run it from a Developer Command Prompt.
'@

$mode = @('--release')
$install = $false
$updateSeed = $false
$dist = $false

foreach ($arg in $args) {
  switch ($arg) {
    '--debug' { $mode = @() }
    '--install' { $install = $true }
    '--update-seed' { $updateSeed = $true }
    '--dist' { $dist = $true }
    { $_ -eq '--help' -or $_ -eq '-h' } { Write-Output $usage; exit 0 }
    default { [Console]::Error.WriteLine("error: unknown option ``$arg```n`n$usage"); exit 2 }
  }
}

if ($dist -and $mode.Count -eq 0) {
  [Console]::Error.WriteLine('error: `--dist` builds a release; it does not take `--debug`')
  exit 2
}

if ($dist -and $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
  [Console]::Error.WriteLine("error: releases for Windows are built for x86-64, not ``$env:PROCESSOR_ARCHITECTURE``")
  exit 1
}

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $env:SAG_CC) {
  $env:SAG_CC = 'clang'
}

$cc = $env:SAG_CC

function Step($text) {
  Write-Output "==> $text"
}

# Runs a program and stops the script if it fails.
function Invoke($program, [string[]]$arguments) {
  & $program @arguments

  if ($LASTEXITCODE -ne 0) {
    [Console]::Error.WriteLine("error: ``$program`` failed with exit code $LASTEXITCODE")
    exit 1
  }
}

# Fails if a check of `file` reports anything: the compiler, Nova and the harness have no warnings.
function CheckClean($file) {
  $report = & cmd /c "build\bin\sag.exe check $file 2>&1"
  $code = $LASTEXITCODE

  if ($report) {
    [Console]::Error.WriteLine(($report -join "`n"))
  }

  if ($code -ne 0) {
    exit 1
  }

  if ($report) {
    [Console]::Error.WriteLine("error: ``$file`` has warnings")
    exit 1
  }
}

foreach ($dir in @('build\stage0', 'build\stage1')) {
  if (Test-Path $dir) {
    Remove-Item -Recurse -Force $dir
  }
}

foreach ($dir in @('build\stage0', 'build\stage1', 'build\bin')) {
  New-Item -ItemType Directory -Force $dir | Out-Null
}

Step "stage 0: the seed, with $cc"
$name = [IO.Path]::GetFileNameWithoutExtension($cc).ToLowerInvariant()
$sources = @('bootstrap\sag.c', 'bootstrap\sagitario_runtime.c')

# The flags `sag` uses for the generated C in release mode, and the stack it reserves on Windows,
# the default of Linux and macOS. `cl` optimizes with `/O1`: its `/O2` miscompiles the compiler.
if ($name -eq 'cl') {
  Invoke $cc (@('/nologo', '/std:c11', '/W3', '/utf-8', '/O1','/DSAG_RELEASE', '/I', 'bootstrap', '/Fobuild\stage0\', '/Febuild\stage0\sag.exe') + $sources + @('/link', '/STACK:8388608'))
} else {
  $machine = & $cc -dumpmachine
  $stack = if ($machine -match 'mingw|gnu') { '-Wl,--stack,8388608' } else { '-Wl,/STACK:8388608' }

  Invoke $cc (@('-std=c11', '-pedantic-errors', '-O2', '-DSAG_RELEASE', '-I', 'bootstrap', '-o', 'build\stage0\sag.exe') + $sources + @($stack))
}

Step 'stage 1'
Invoke 'build\stage0\sag.exe' (@('build') + $mode + @('compiler/src/main.sag', '--output', 'build\stage1\sag.exe'))

Step 'stage 2'
Invoke 'build\stage1\sag.exe' (@('build') + $mode + @('compiler/src/main.sag', '--output', 'build\bin\sag.exe'))

Step 'fixed point: stages 1 and 2 generate the same C'
# Through cmd, which writes the bytes of the C as they are.
Invoke 'cmd' @('/c', 'build\stage1\sag.exe check --dump=c compiler/src/main.sag > build\stage1\sag.c')
Invoke 'cmd' @('/c', 'build\bin\sag.exe check --dump=c compiler/src/main.sag > build\sag.c')

$first = Get-FileHash 'build\stage1\sag.c'
$second = Get-FileHash 'build\sag.c'

if ($first.Hash -ne $second.Hash) {
  [Console]::Error.WriteLine('error: stages 1 and 2 generate different C for the compiler (build\stage1\sag.c, build\sag.c)')
  exit 1
}

Step 'no warnings in the compiler, Nova and the harness'
CheckClean 'compiler/src/main.sag'
CheckClean 'compiler/src/nova_main.sag'
CheckClean 'tests/harness/main.sag'

Step 'nova'
Invoke 'build\bin\sag.exe' (@('build') + $mode + @('compiler/src/nova_main.sag', '--output', 'build\bin\nova.exe'))

if ($updateSeed) {
  Step 'update the seed'
  Copy-Item 'build\sag.c' 'bootstrap\sag.c'
  Copy-Item 'runtime\sagitario_runtime.c', 'runtime\sagitario_runtime.h' 'bootstrap\'
}

if ($install) {
  $bin = Join-Path $env:USERPROFILE '.sagitario\bin'

  Step "install to $bin"
  New-Item -ItemType Directory -Force $bin | Out-Null

  # A running executable cannot be overwritten on Windows, but it can be renamed.
  foreach ($tool in @('sag', 'nova')) {
    $target = Join-Path $bin "$tool.exe"
    $old = Join-Path $bin "$tool.old.exe"

    if (Test-Path $old) {
      Remove-Item -Force $old -ErrorAction SilentlyContinue
    }

    if (Test-Path $target) {
      Move-Item -Force $target $old
    }

    Copy-Item "build\bin\$tool.exe" $target
  }

  $path = [Environment]::GetEnvironmentVariable('Path', 'User')
  $entries = if ($path) { $path -split ';' } else { @() }

  if ($entries -notcontains $bin) {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $bin) | Where-Object { $_ }) -join ';', 'User')
    Write-Output "added $bin to the PATH of the user; open a new terminal to use sag and nova"
  }
}

if ($dist) {
  # The archive of a release (docs/adr/0068-releases-and-distribution.md): `sag` and `nova` in
  # `bin\`, with the licenses and the README, named after the version and the host.
  $version = ((& 'build\bin\sag.exe' --version) -split ' ')[1]
  $name = "sagitario-$version-x86_64-pc-windows-msvc"
  $staging = "build\dist\$name"

  Step "archive build\dist\$name.zip"

  if (Test-Path 'build\dist') {
    Remove-Item -Recurse -Force 'build\dist'
  }

  New-Item -ItemType Directory -Force "$staging\bin" | Out-Null
  Copy-Item 'build\bin\sag.exe', 'build\bin\nova.exe' "$staging\bin\"
  Copy-Item 'LICENSE-MIT', 'LICENSE-APACHE', 'README.md' "$staging\"

  # ZipFile writes `/` between the directories of an entry, as every unzip expects.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::CreateFromDirectory((Join-Path $root $staging), (Join-Path $root "build\dist\$name.zip"), 'Optimal', $true)
}

Step 'done: build\bin\sag.exe and build\bin\nova.exe'
