#include <stdexcept>
#include <iostream>

#include "simplekv.hpp"
#include "pvar.hpp"

template <typename Value, std::size_t N>
const Value &simple_kv<Value, N>::get(const std::string &key) {
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

template <typename Value, std::size_t N>
void simple_kv<Value, N>::put(const std::string &key, const Value &val) {
    auto index = std::hash<std::string>{}(key) % N;

    /* search for element with specified key - if found
        * transactionally update its value */
    for (const auto &e: buckets[index]) {
        if (e.first == key)
        {
            PTx { values[e.second] = val; }
            return;
        }
    }

    /* if there is no element with specified key, insert new value
        * to the end of values vector and put reference in proper
        * bucket transactionally */
    auto elem = std::pair<key_type, std::size_t>(key, values.size() - 1);
    PTx {
        values.push_back(val);        
        buckets[index].push_back(elem);
    }
}

void show_usage(char *argv[])
{
    std::cerr << "usage: " << argv[0]
              << " [get key|put key value] |"
              << " [burst get|put|putget count]" << std::endl;
}

void initialize() {
    int found = 0;
    PTx {
        if (PGET(kv) == NULL) {
            kv_t *k = (kv_t*)pmalloc(sizeof(kv_t));
            k->init();
            PSET(kv, k);
        } else {
            found = 1;
        }
    }
    if (!found) {
        fprintf(stderr, "Created the root object.\n");
    } else {
        fprintf(stderr, "Found the root object.\n");
    }
}

int main(int argc, char *argv[])
{
    if (argc < 2)
    {
        show_usage(argv);
        return 1;
    }

    initialize();

    if (std::string(argv[1]) == "get" && argc == 3) {
        kv_t *k = NULL;
        PTx { k = PGET(kv); }
        assert(k);
        std::cout << k->get(argv[2]) << std::endl;
    } else if (std::string(argv[1]) == "put" && argc == 4) {
        kv_t *k = NULL;
        PTx { k = PGET(kv); }
        assert(k);
        k->put(argv[2], std::stoi(argv[3]));
    } else if (std::string(argv[1]) == "burst" && std::string(argv[2]) == "get" && argc == 4) {
        kv_t *k = NULL;
        PTx { k = PGET(kv); }
        assert(k);
        int m = std::stoi(argv[3]);
        for (int i = 0; i < m; i++)
        {
            char key[32] = {0};
            sprintf(key, "key%d", i);
            k->get(key);
        }
    } else if (std::string(argv[1]) == "burst" && std::string(argv[2]) == "put" && argc == 4) {
        kv_t *k = NULL;
        PTx { k = PGET(kv); }
        assert(k);
        int m = std::stoi(argv[3]);
        fprintf(stderr, "inserting %d items...\n", m);
        for (int i = 0; i < m; i++)
        {
            char key[32] = {0};
            sprintf(key, "key%d", i);
            k->put(key, i);
        }
    } else {
        show_usage(argv);
    }

    return 0;
}

