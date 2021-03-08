#ifndef __KV_H__
#define __KV_H__

#include <functional>
#include <stdexcept>
#include <string>
#include "vector.hpp"

/**
 * Value - type of the value stored in hashmap
 * N - number of buckets in hashmap
 */
template <typename Value, std::size_t N>
class simple_kv
{
private:
    using key_type = fix_string;
    using bucket_type = vector<std::pair<key_type, std::size_t>>;
    using value_vector = vector<Value>;

    bucket_type buckets[N];
    value_vector values;
public:
    simple_kv() {}

    void init() {
        values.init();
        for (int i=0; i<N; i++) {
            buckets[i].init();
        }
    }

    const Value &get(const std::string &key);
    void put(const std::string &key, const Value &val);
};

#endif /* __KV_H__ */