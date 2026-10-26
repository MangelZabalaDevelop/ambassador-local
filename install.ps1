# Ambassador Local installer for Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/MangelZabalaDevelop/ambassador-local/main/install.ps1 | iex
#
# Downloads the Windows installer from the latest GitHub release, checks its SHA256
# against the one GitHub publishes for that file, and installs it for the current user (no admin).
& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'  # Invoke-WebRequest is many times faster without its progress bar
  $Repo = 'MangelZabalaDevelop/ambassador-local'

  function Say($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
  function Fail($msg) { Write-Host "Error: $msg" -ForegroundColor Red }

  if (-not [Environment]::Is64BitOperatingSystem) { Fail 'Ambassador Local needs 64-bit Windows.'; return }
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12  # Windows PowerShell 5.1

  # While the repository is private, GitHub only answers to a signed-in user: use the GitHub CLI login if there is one.
  $headers = @{ 'Accept' = 'application/vnd.github+json'; 'User-Agent' = 'ambassador-local-installer' }
  $token = $env:GITHUB_TOKEN
  if (-not $token -and (Get-Command gh -ErrorAction SilentlyContinue)) { $token = (& gh auth token 2>$null) }
  if ($token) { $headers['Authorization'] = "Bearer $token" }

  Say 'Looking for the latest Ambassador Local release'
  try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
  } catch {
    Fail 'Could not reach the release on GitHub. If the repository is private, sign in first with: gh auth login'; return
  }
  $asset = $release.assets | Where-Object { $_.name -like '*_x64-setup.exe' } | Select-Object -First 1
  if (-not $asset) { Fail 'The latest release has no Windows installer.'; return }
  if ("$($asset.digest)" -notlike 'sha256:*') { Fail "GitHub did not publish a SHA256 for $($asset.name), so it cannot be verified."; return }
  $want = $asset.digest.Substring(7).ToLower()

  $setup = Join-Path $env:TEMP 'ambassador-local-setup.exe'
  Say "Downloading $($asset.name) ($($release.tag_name), $([math]::Round($asset.size / 1MB)) MB)"
  $download = $headers.Clone()
  $download['Accept'] = 'application/octet-stream'
  try {
    Invoke-WebRequest -Uri $asset.url -Headers $download -OutFile $setup -UseBasicParsing
  } catch {
    Fail 'Download failed.'; return
  }

  Say 'Checking SHA256'
  $have = (Get-FileHash -Path $setup -Algorithm SHA256).Hash.ToLower()
  if ($have -ne $want) {
    Remove-Item $setup -Force
    Fail "SHA256 does not match (expected $want, got $have). Nothing was installed."; return
  }

  Say 'Installing'
  $p = Start-Process -FilePath $setup -ArgumentList '/S' -Wait -PassThru
  Remove-Item $setup -Force
  if ($p.ExitCode -ne 0) { Fail "The installer exited with code $($p.ExitCode)."; return }

  Say 'Done. Open Ambassador Local from the Start menu.'
}
