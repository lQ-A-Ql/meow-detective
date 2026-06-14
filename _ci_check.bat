@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >/dev/null 2>/dev/null
cd /d D:\process\forensic
cargo check --workspace 2>&1
