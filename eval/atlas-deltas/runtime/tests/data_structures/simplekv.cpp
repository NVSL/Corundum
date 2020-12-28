#include <stdexcept>
#include <iostream>

#include "simplekv.hpp"
#include "atlas_alloc.h"
#include "atlas_api.h"

using kv_t = simple_kv<int, 10>;

kv_t *KV;
// ID of Atlas persistent region
uint32_t kv_rgn_id;

void show_usage(char *argv[])
{
    std::cerr << "usage: " << argv[0]
              << " [get key|put key value]" << std::endl;
}

void initialize()
{
    void *rgn_root = NVM_GetRegionRoot(kv_rgn_id);
    if (rgn_root)
    {
        KV = (kv_t *)rgn_root;
        KV->init(kv_rgn_id);

        fprintf(stderr, "Found kv at %p\n", (void *)KV);
    }
    else
    {
        KV = (kv_t *)nvm_alloc(sizeof(kv_t), kv_rgn_id);
        fprintf(stderr, "Created KV at %p\n", (void *)KV);

        KV->init(kv_rgn_id);

        NVM_BEGIN_DURABLE();

        // Set the root of the Atlas persistent region
        NVM_SetRegionRoot(kv_rgn_id, KV);

        NVM_END_DURABLE();
    }
}

int main(int argc, char *argv[])
{
    if (argc < 2)
    {
        show_usage(argv);
        return 1;
    }

    // Initialize Atlas
    NVM_Initialize();
    // Create an Atlas persistent region
    kv_rgn_id = NVM_FindOrCreateRegion("kv", O_RDWR, NULL);
    // This contains the Atlas restart code to find any reusable data
    initialize();

    if (std::string(argv[1]) == "get" && argc == 3)
        std::cout << KV->get(argv[2]) << std::endl;
    else if (std::string(argv[1]) == "put" && argc == 4)
        KV->put(argv[2], std::stoi(argv[3]));
    else if (std::string(argv[1]) == "burst" && std::string(argv[2]) == "get" && argc == 4)
    {
        int m = std::stoi(argv[3]);
        for (int i = 0; i < m; i++)
        {
            char key[32] = {0};
            sprintf(key, "key%d", i);
            KV->get(key);
        }
    }
    else if (std::string(argv[1]) == "burst" && std::string(argv[2]) == "put" && argc == 4)
    {
        int m = std::stoi(argv[3]);
        printf("inserting %d items...\n", m);
        for (int i = 0; i < m; i++)
        {
            char key[32] = {0};
            sprintf(key, "key%d", i);
            KV->put(key, i);
        }
    }
    else
    {
        show_usage(argv);
    }

    // Close the Atlas persistent region
    NVM_CloseRegion(kv_rgn_id);
    // Atlas bookkeeping
    NVM_Finalize();
    return 0;
}

