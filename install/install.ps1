# Installs `sag` and `nova` from a release of Sagitario (docs/adr/0069-installers.md). Windows;
# elsewhere, `install.sh`.
#
#   irm https://github.com/Juanestban/sagitario/releases/latest/download/install.ps1 | iex
#
# Run as a script it takes -Version X.Y.Z, -Dir DIR (default: %USERPROFILE%\.sagitario),
# -NoModifyPath and -From URL (a directory with the archives and SHA256SUMS, instead of the GitHub
# Release). Through `iex`, which takes no arguments, SAGITARIO_VERSION chooses the release.
param(
  [string]$Version = $env:SAGITARIO_VERSION,
  [string]$Dir = (Join-Path $env:USERPROFILE '.sagitario'),
  [switch]$NoModifyPath,
  [string]$From = ''
)

# Everything happens in a function and fails by throwing: through `iex` the script runs in the
# session of the user, which `exit` would close.
function Install-Sagitario([string]$Version, [string]$Dir, [bool]$ModifyPath, [string]$From) {
  $ErrorActionPreference = 'Stop'
  $repository = 'https://github.com/Juanestban/sagitario'

  # The releases for Windows are for x86-64 (ADR-0068, Q381).
  $arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture
  if ($arch -ne [Runtime.InteropServices.Architecture]::X64) {
    throw "the releases for Windows are for x86-64, not $arch; build from source ($repository)"
  }

  $triple = 'x86_64-pc-windows-msvc'

  # The curl of the system, since Windows 10 1803 (ADR-0068, Q386); `curl` alone is an alias of
  # Invoke-WebRequest in Windows PowerShell.
  if (-not (Get-Command 'curl.exe' -ErrorAction SilentlyContinue)) {
    throw 'the installer needs curl.exe, which Windows 10 (1803) and later ship'
  }

  $base = if ($From) {
    $From.TrimEnd('/')
  } elseif ($Version) {
    "$repository/releases/download/v$Version"
  } else {
    "$repository/releases/latest/download"
  }

  $tmp = Join-Path ([IO.Path]::GetTempPath()) "sagitario-$([guid]::NewGuid())"
  New-Item -ItemType Directory $tmp | Out-Null

  try {
    function Get-File([string]$url, [string]$file) {
      & curl.exe --proto '=https,file' -fsSL -o $file $url
      if ($LASTEXITCODE -ne 0) {
        throw "cannot download $url"
      }
    }

    # The checksums of a release name its archives, and so the version of the latest one.
    Get-File "$base/SHA256SUMS" (Join-Path $tmp 'SHA256SUMS')

    $expected = $null
    $archive = $null

    foreach ($line in Get-Content (Join-Path $tmp 'SHA256SUMS')) {
      if ($line -match '^([0-9a-fA-F]{64}) [ *](sagitario-\S+)$' -and $Matches[2].EndsWith("-$triple.zip")) {
        $expected = $Matches[1]
        $archive = $Matches[2]
      }
    }

    if (-not $archive) {
      throw "the release has no archive for $triple"
    }

    if ($Version -and $archive -ne "sagitario-$Version-$triple.zip") {
      throw "the release of $Version has no archive sagitario-$Version-$triple.zip"
    }

    Write-Output "downloading $archive"
    $zip = Join-Path $tmp $archive
    Get-File "$base/$archive" $zip

    if ((Get-FileHash -Algorithm SHA256 $zip).Hash -ne $expected.ToUpperInvariant()) {
      throw "the checksum of $archive does not match SHA256SUMS"
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $tmp)
    $release = Join-Path $tmp ([IO.Path]::GetFileNameWithoutExtension($archive))

    foreach ($tool in @('sag', 'nova')) {
      if (-not (Test-Path (Join-Path $release "bin\$tool.exe"))) {
        throw "$archive does not hold bin\$tool.exe"
      }
    }

    New-Item -ItemType Directory -Force (Join-Path $Dir 'bin') | Out-Null
    $bin = (Resolve-Path (Join-Path $Dir 'bin')).Path

    # As `bootstrap/build.ps1 --install` does: a running executable cannot be overwritten on
    # Windows, but it can be renamed.
    foreach ($tool in @('sag', 'nova')) {
      $target = Join-Path $bin "$tool.exe"
      $old = Join-Path $bin "$tool.old.exe"

      if (Test-Path $old) {
        Remove-Item -Force $old -ErrorAction SilentlyContinue
      }

      if (Test-Path $target) {
        Move-Item -Force $target $old
      }

      Copy-Item (Join-Path $release "bin\$tool.exe") $target
    }
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }

  Write-Output "installed $(& (Join-Path $bin 'sag.exe') --version) and $(& (Join-Path $bin 'nova.exe') --version) to $bin"

  if ($ModifyPath) {
    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = if ($path) { $path -split ';' } else { @() }

    if ($entries -notcontains $bin) {
      [Environment]::SetEnvironmentVariable('Path', (($entries + $bin) | Where-Object { $_ }) -join ';', 'User')
      Write-Output "added $bin to the PATH of the user"
    }

    # Through `iex`, also the terminal that ran the installer.
    if (($env:Path -split ';') -notcontains $bin) {
      $env:Path = "$bin;$env:Path"
    }
  }

  # `sag` compiles through a C compiler, which it looks for as here (ADR-0068, Q388).
  $candidates = if ($env:SAG_CC -and $env:SAG_CC.Trim()) { @($env:SAG_CC) } else { @('clang', 'gcc') }
  $compiler = $null
  $target = $null

  foreach ($candidate in $candidates) {
    if (-not (Get-Command $candidate -ErrorAction SilentlyContinue)) {
      continue
    }

    if ([IO.Path]::GetFileNameWithoutExtension($candidate) -eq 'cl') {
      $compiler = $candidate
      $target = 'msvc'
      break
    }

    $output = & $candidate -dumpmachine 2>$null
    if ($LASTEXITCODE -eq 0) {
      $compiler = $candidate
      $target = "$output".Trim()
      break
    }
  }

  $llvm = Join-Path $env:ProgramFiles 'LLVM\bin\clang.exe'
  $buildTools = 'winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"'

  if (-not $compiler) {
    if ($env:SAG_CC -and $env:SAG_CC.Trim()) {
      Write-Warning "cannot run the C compiler ``$env:SAG_CC`` (from SAG_CC), which sag needs"
    } elseif (Test-Path $llvm) {
      Write-Warning "LLVM is installed but clang is not on PATH; add $(Split-Path $llvm) to PATH, or set SAG_CC to $llvm"
    } else {
      Write-Warning "no C compiler found; sag needs clang on PATH, or SAG_CC. To install LLVM and the Visual Studio Build Tools (its linker and C library):`n  winget install LLVM.LLVM`n  $buildTools"
    }
    return
  }

  Write-Output "sag will compile with $compiler ($target)"

  if ($target -like '*windows-gnu') {
    Write-Warning "MinGW is supported best-effort and without sanitizers; the official LLVM (winget install LLVM.LLVM) targets x86_64-pc-windows-msvc (docs/adr/0018-windows-c-toolchain.md)"
  }

  # The LLVM clang for Windows links with the tools and the C library of Visual Studio.
  if ($target -like '*windows-msvc') {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $tools = if (Test-Path $vswhere) {
      & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }

    if (-not $tools) {
      Write-Warning "clang needs the C++ build tools of Visual Studio, which were not found. To install them:`n  $buildTools"
    }
  }
}

Install-Sagitario -Version $Version -Dir $Dir -ModifyPath (-not $NoModifyPath) -From $From
