#ifndef PROG_H_
#define PROG_H_

#include <fstream>
#include <sstream>
#include <iostream>
#include <iterator>
#include <vector>
#include <string>
#include <functional>

using namespace std;

string next_op(vector<string> &args, int &i) {
    if (i < args.size()) {
        return args[i++];
    } else {
        return "";
    }
}

bool repeat(vector<string> &args, int i, int n, function<bool(vector<string>)> exec) {
    vector<string> v;
    v.assign(args.begin() + i, args.end());
    v.insert(v.begin(), "");
    v.insert(v.begin(), "");
    while ( n-- > 1 ) {
        if ( !exec(v) ) {
            return false;
        }
    }
    return true;
}

bool run(string filename, function<bool(vector<string>)> exec) {
    ifstream ifs(filename);
    string content((istreambuf_iterator<char>(ifs)),
                    (istreambuf_iterator<char>()));
    
    istringstream iss(content);
    vector<string> args(istream_iterator<string>{iss},
                        istream_iterator<string>());
                            
    args.insert(args.begin(), "");
    args.insert(args.begin(), "");
    
    return exec(args);
}

#include <cstdarg>

std::string format(const char *fmt...) {
	char str[128];
    va_list args;
    va_start(args, fmt);
	vsprintf(str, fmt, args);
    va_end(args);

	return str;
}


#include <cstdint>

#ifndef _WIN32

#include <unistd.h>

#define CREATE_MODE_RW (S_IWUSR | S_IRUSR)

/*
 * file_exists -- checks if file exists
 */
static inline int
file_exists(char const *file)
{
	return access(file, F_OK);
}

/*
 * find_last_set_64 -- returns last set bit position or -1 if set bit not found
 */
static inline int
find_last_set_64(uint64_t val)
{
	return 64 - __builtin_clzll(val) - 1;
}

#else

#include <corecrt_io.h>
#include <process.h>
#include <windows.h>

#define CREATE_MODE_RW (S_IWRITE | S_IREAD)

/*
 * file_exists -- checks if file exists
 */
static inline int
file_exists(char const *file)
{
	return _access(file, 0);
}

/*
 * find_last_set_64 -- returns last set bit position or -1 if set bit not found
 */
static inline int
find_last_set_64(uint64_t val)
{
	DWORD lz = 0;

	if (BitScanReverse64(&lz, val))
		return (int)lz;
	else
		return -1;
}

#endif

#endif