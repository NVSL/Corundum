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

    const Value &
    get(const std::string &key)
    {
        auto index = std::hash<std::string>{}(key) % N;

        fix_string pkey(key);
        for (int i=0; i<buckets[index].size(); i++)
        {
            auto e = buckets[index][i];
            if (e.first == pkey) {
                return values[e.second];
            }
        }

        char msg[120];
        sprintf(msg, "no entry in simplekv for `%s`", key.c_str());
        throw std::out_of_range(msg);
    }

    void
    put(const std::string &key, const Value &val)
    {
        auto index = std::hash<std::string>{}(key) % N;
        pthread_mutex_lock(this->lock);

        fix_string pkey(key);
        /* search for element with specified key - if found
		 * transactionally update its value */
        for (int i=0; i<buckets[index].size(); i++)
        {
            auto e = buckets[index][i];
            if (e.first == pkey)
            {
                // printf("key (%s, %s) found at %d with value %d; new value is %d\n", pkey.c_str(), e.first.c_str(), values.size() - 1, values[e.second], val);
                values[e.second] = val;
                pthread_mutex_unlock(this->lock);
                return;
            }
        }

        /* if there is no element with specified key, insert new value
		 * to the end of values vector and put reference in proper
		 * bucket transactionally */
        values.push(val, kv_rgn_id);        
        // printf("new key %s and value %d pair at %d\n", pkey.c_str(), val, values.size() - 1);
        buckets[index].push(std::pair<key_type, std::size_t>(pkey, values.size() - 1), kv_rgn_id);
        pthread_mutex_unlock(this->lock);
    }
};

