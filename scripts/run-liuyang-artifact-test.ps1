$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cmd /d /s /c "call `"$vcvars`" && cd /d D:\process\forensic && cargo test -p app-services --test e01_liuyang_regression_test liuyang_e01_artifact_extraction -- --ignored --nocapture 2>&1"
