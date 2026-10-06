B = ../../crates/bwapi

# bwapi_c.h is a C header: the bot compiles it as strict C.
CFLAGS = -std=c11 -pedantic -Wall -Wextra -Werror

libbot_c.so: bot.c bwapi_c.o
	gcc $(CFLAGS) -shared -fPIC -o $@ -I $(B)/cpp/include bot.c bwapi_c.o -lstdc++

# The C++ implementation of bwapi_c.h.
bwapi_c.o: $(wildcard $(B)/cpp/*.cpp $(B)/cpp/*.h $(B)/cpp/include/*.h $(B)/cpp/generated/*.cpp)
	g++ -r -fPIC -o $@ -isystem $(B)/vendor/bwapi/bwapi/include $(filter %.cpp,$^)

clean:
	rm -f libbot_c.so bwapi_c.o

.PHONY: clean
