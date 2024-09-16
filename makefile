# broken linux makefile, committed for testing and posterity. do not use -- ali

make:
	x86_64-w64-mingw32-g++ -c -pipe -fvisibility=hidden -O2 -fmessage-length=0 -D_FORTIFY_SOURCE=2 -fstack-protector -funwind-tables -fasynchronous-unwind-tables -W -fPIC -DXPLM200 -DXLPM210 -DXPLM300 -DXPLM301 -DXPLM303 -DAPL=0 -DIBM=1 -DLIN=0 -I../SDK/CHeaders/XPLM -I../SDK/CHeaders/Widgets -I. -I/usr/local/include -o HelloWorld.o hello_world.cpp;
	x86_64-w64-mingw32-g++ -Wl,-O1 -shared -L/usr/include/GL -L/mnt/c/ -L../SDK/Libraries/Win -o HelloWorld.xpl HelloWorld.o -lXPLM_64 -lXPWidgets_64;
	# x86_64-w64-mingw32-g++ -c -pipe -fvisibility=hidden -O2 -fmessage-length=0 -D_FORTIFY_SOURCE=2 -fstack-protector -funwind-tables -fasynchronous-unwind-tables -W -fPIC -DXPLM200 -DXLPM210 -DXPLM300 -DXPLM301 -DXPLM303 -DXPLM400 -DXPLM410 -DAPL=0 -DIBM=1 -DLIN=0 -I../SDK/CHeaders/XPLM -I../SDK/CHeaders/Widgets -I. -I/usr/local/include -L../SDK/Libraries/Win -o HelloWorld.xpl hi_world.cpp;
	# C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64\
	# -L/mnt/c/Program\ Files\ \(x86\)/Windows\ Kits/10/Lib/10.0.26100.0/um/x64/OpenGL32.Lib
