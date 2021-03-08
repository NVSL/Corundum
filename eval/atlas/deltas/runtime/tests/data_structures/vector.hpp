#ifndef __NVM_VECTOR
#define __NVM_VECTOR

#include <functional>
#include <stdexcept>
#include <string.h>
#include "atlas_alloc.h"
#include "atlas_api.h"

class string;

template <typename T>
class vector
{
private:
    int capacity;
    int len;
    T *data;

public:
    vector(): capacity(0), len(0), data(NULL) {};
    vector(int cap, uint32_t rgn_id): capacity(cap), len(0) {
        data = (T*)nvm_alloc(sizeof(T)*capacity, rgn_id);
    }

    inline void
    push_back(const T &val, uint32_t rgn_id)
    {
        if (len == capacity) {
            capacity = std::max(1,capacity*2);
            T *n = (T*)nvm_alloc(sizeof(T)*capacity, rgn_id);
            if (data) {
                memcpy(n, data, sizeof(T)*len);
                nvm_free(data);
            }
            data = n;
        }
        data[len++] = val;
    }

    inline int size() const {return len;}

    inline T operator [](int idx) const {return data[idx];}
    inline T & operator [](int idx) {return data[idx];}

    typedef T * iterator;
    iterator begin() { return data; }
    iterator end() { return data + len; }

    friend class string;
};

class string 
{
private:
    vector<char> vec;
    uint32_t rgn_id;

public:
    string(uint32_t rgn_id): vec(), rgn_id(rgn_id) {};
    string(const std::string &s, uint32_t rgn_id): vec(s.length(), rgn_id), rgn_id(rgn_id) {
        memcpy(vec.data, s.c_str(), s.length());
        vec.len = s.length();
    }

    inline const char* c_str() {
        return vec.data;
    }

    inline std::string s_str() {
        return std::string(vec.data, vec.len);
    }

    inline void operator+=(const char*a) {
        int i=0;
        while (a[i]) {
            vec.push_back(a[i++], rgn_id);
        }
    }

    inline bool operator==(const string &other) {
        if (vec.len != other.vec.len) return false;
        return strcmp(vec.data, other.vec.data) == 0;
    }

    inline bool operator==(const std::string &other) {
        return other.compare(vec.data) == 0;
    }
};

class fix_string 
{
private:
    char data[32];
    int len;

public:
    fix_string() {data[0] = '\0'; len=0;}
    fix_string(const std::string &s) {
        len = std::min(31lu,s.length());
        memcpy(data, s.c_str(), len);
        data[len] = '\0';
    }

    inline const char* c_str() {
        return data;
    }

    inline std::string s_str() {
        return std::string(data, len);
    }

    inline bool operator==(const std::string &other) const {
        return other.compare(data) == 0;
    }
};

#endif // __NVM_VECTOR