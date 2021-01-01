/*
 * simplekv.hpp -- implementation of simple kv which uses vector to hold
 * values, string as a key and array to hold buckets
 */

#include <functional>
#include <stdexcept>
#include <string>
#include <vector>
#include <array>
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
    uint32_t kv_rgn_id;
    pthread_mutex_t *lock;

public:
    simple_kv(uint32_t kv_rgn_id): kv_rgn_id(kv_rgn_id) {}

    void init(uint32_t kv_rgn_id) {
        this->kv_rgn_id = kv_rgn_id;
        this->lock = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
        pthread_mutex_init(this->lock, NULL);
    }

    const Value &get(const std::string &key) {
        auto index = std::hash<std::string>{}(key) % N;

        for (const auto &e: buckets[index]) {
            if (e.first == key) {
                return values[e.second];
            }
        }

        char msg[120];
        sprintf(msg, "no entry in simplekv for `%s`", key.c_str());
        throw std::out_of_range(msg);
    }

    void put(const std::string &key, const Value &val) {
        auto index = std::hash<std::string>{}(key) % N;
        pthread_mutex_lock(this->lock);

        /* search for element with specified key - if found
		 * transactionally update its value */
        for (const auto &e: buckets[index]) {
            if (e.first == key)
            {
                values[e.second] = val;
                pthread_mutex_unlock(this->lock);
                return;
            }
        }

        /* if there is no element with specified key, insert new value
		 * to the end of values vector and put reference in proper
		 * bucket transactionally */
        values.push_back(val, kv_rgn_id);        
        buckets[index].push_back(std::pair<key_type, std::size_t>(key, values.size() - 1), kv_rgn_id);
        pthread_mutex_unlock(this->lock);
    }
};
