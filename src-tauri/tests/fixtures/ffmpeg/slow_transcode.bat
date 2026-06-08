@echo off
REM Stand-in for ffmpeg used by cancellation tests. Sleeps for SLOW_TRANSCODE_SLEEP
REM seconds (default 10), then exits 0. Honours -y / -i / -ss / -to etc. by
REM ignoring them.
if "%SLOW_TRANSCODE_SLEEP%"=="" set SLOW_TRANSCODE_SLEEP=10
timeout /t %SLOW_TRANSCODE_SLEEP% /nobreak >nul
exit /b 0
