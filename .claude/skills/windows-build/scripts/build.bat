@echo off
rem Copyright (C) 2026 The AiQuokkaWannabe Authors https://github.com/ai-quokka-wannabe
rem
rem This program is free software: you can redistribute it and/or modify it under the terms of
rem the GNU General Public License as published by the Free Software Foundation, either version
rem 3 of the License, or (at your option) any later version.
rem
rem This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
rem without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
rem See the GNU General Public License for more details.
rem
rem You should have received a copy of the GNU General Public License along with this program.
rem If not, see https://www.gnu.org/licenses/.

rem build.bat <source dir> <preset> [install prefix]
rem Configures, builds Release and runs ctest for one CMake preset inside the Visual Studio developer
rem environment (found with vswhere), then optionally installs into a prefix. Prints BUILD-OK at the end.
setlocal
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath`) do set VSROOT=%%i
if not exist "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" ( echo no Visual Studio found & exit /b 1 )
call "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
cd /d "%~1" || exit /b 1
cmake --preset %2 >nul || exit /b 2
cmake --build build\%2 --config Release || exit /b 3
ctest --test-dir build\%2 -C Release --output-on-failure || exit /b 4
if not "%~3"=="" ( cmake --install build\%2 --config Release --prefix "%~3" || exit /b 5 )
echo BUILD-OK
