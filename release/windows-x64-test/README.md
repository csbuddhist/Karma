# Karma Windows x64 test bundle

This directory is a cloneable Windows test bundle for the current Karma Agent. It is unsigned development/test software, not an installer and not a complete family-control product.

## Run on Windows

1. Install the Microsoft Visual C++ 2015--2022 Redistributable for x64.
2. Clone this repository and open PowerShell in this directory.
3. If Windows blocks the unsigned local script, use `Set-ExecutionPolicy -Scope Process Bypass` for the current PowerShell process only.
4. Run `./Start-KarmaConsole.ps1` to open the password-protected administration UI.
5. Run `./Start-KarmaTest.ps1` in another PowerShell window to start the screen-monitoring Agent.

The launcher validates every shipped executable, DLL, and model asset before it starts the Agent. It sets model configuration only in the launched process. You may select OCR behavior with `./Start-KarmaTest.ps1 -OcrProfile lightweight`.

## Test scope and limitations

This bundle includes the Windows x64 administration UI, Agent, DirectML runtime DLL, Viddexa image model, and PP-OCRv5 mobile OCR assets. The UI can authenticate an administrator and edit local settings, but it is not yet connected to the Agent through the planned Windows Service. It is unsigned and intended only for controlled functional testing. It does not install a Windows service, automatically close applications, resist tampering, enforce schedules, store encrypted event images, or provide a production installer.
