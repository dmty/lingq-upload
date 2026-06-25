@echo off
REM Stand-in for ffmpeg used by cancellation tests. Sleeps for SLOW_TRANSCODE_SLEEP
REM seconds (default 10), then exits 0. Honours -y / -i / -ss / -to etc. by
REM ignoring them. timeout.exe aborts under redirected stdin in CI runners —
REM PowerShell's Start-Sleep is console-independent.
if "%SLOW_TRANSCODE_SLEEP%"=="" set SLOW_TRANSCODE_SLEEP=10
powershell -NoProfile -NonInteractive -Command "Start-Sleep -Seconds %SLOW_TRANSCODE_SLEEP%"
exit /b 0
