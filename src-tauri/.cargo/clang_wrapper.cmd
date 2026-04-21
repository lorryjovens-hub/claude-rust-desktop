@echo off
set "FILTERED_ARGS="
:parse_args
if "%~1"=="" goto done
set "arg=%~1"
if "%arg%"=="-nologo" goto next
if "%arg%"=="/Gy" goto next
if "%arg%"=="/Zc:wchar_t" goto next
if "%arg%"=="/Zc:forScope" goto next
if "%arg%"=="/Zc:inline" goto next
if "%arg%"=="/Wall" goto next
if "%arg:~0,3%"=="/wd" goto next
if "%arg%"=="/Gy" goto next
if "%arg%"=="/MP" goto next
if "%arg:~0,2%"=="/Z" goto next
if "%arg%"=="-Wl,-dll" goto next
if "%arg%"=="-Wl,--dynamicbase" goto next
if "%arg%"=="-Wl,--high-entropy-va" goto next
if "%arg%"=="-Wl,--nxcompat" goto next
if "%arg%"=="-debug" goto next
if "%arg%"=="/LTCG" goto next
if "%arg%"=="/PROFILE" goto next
if "%arg%"=="/OPT:REF" goto next
if "%arg%"=="/OPT:ICF" goto next
set "FILTERED_ARGS=!FILTERED_ARGS! %arg%"
:next
shift
goto parse_args
:done

"C:\Users\user\AppData\Local\Android\Sdk\ndk\29.0.13113456\toolchains\llvm\prebuilt\windows-x86_64\bin\clang.exe" --target=aarch64-linux-android21 %FILTERED_ARGS%
