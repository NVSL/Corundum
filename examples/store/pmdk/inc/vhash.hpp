#include <functional>
#include <stdexcept>
#include <string>
#include "prog.hpp"

using namespace std;

#define BUCKETS_NUM 10

class vhash {
private:
	using key_type = string;
	using bucket_type = vector<pair<key_type, size_t>>;
	using bucket_array_type = array<bucket_type, BUCKETS_NUM>;
	using value_vector = vector<uint64_t>;

	bucket_array_type buckets;
	value_vector values;

public:
	vhash() = default;

	const uint64_t & get(const string &key) const {
		auto index = hash<string>{}(key) % BUCKETS_NUM;

		for (const auto &e : buckets[index]) {
			if (e.first == key)
				return values[e.second];
		}

		throw out_of_range("no entry");
	}

	void put(const string &key, const uint64_t &val) {
		auto index = hash<string>{}(key) % BUCKETS_NUM;

		for (const auto &e : buckets[index]) {
			if (e.first == key) {
				values[e.second] = val;
				return;
			}
		}

        values.emplace_back(val);
        buckets[index].emplace_back(key, values.size() - 1);
	}

    void clear() {
        for (size_t i=0; i<BUCKETS_NUM; i++) {
            buckets[i].clear();
        }
        values.clear();
    }

    bool empty() {
        return values.empty();
    }

    string print() {
        string ret;

        for (size_t i=0; i<BUCKETS_NUM; i++) {
            ret += format("Bucket[%d]: { ", i);
            for (const auto &e : buckets[i]) {
                ret += format("(%s, %lu) ", e.first.c_str(), values[e.second]);
            }
            ret += "}\n";
        }
        
        return ret;
    }

    bool exec(vector<string> args) {
		if (args.size() < 2) {
            help();
        } else {
            int i = 2;
            while (i < args.size()) {
				string op = next_op(args, i);
                if (!op.empty()) {
                    if (op == "help") {
                        help();
                    } else if (op == "repeat") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            int m = std::stoi(n);
                            if (!repeat(args, i, m,[&](vector<string> args) {
                                return exec(args);
                            } )) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if (op == "put") {
						string key = next_op(args, i);
                        if (!key.empty()) {
						    string sval = next_op(args, i);
                            if (!sval.empty()) {
                                uint64_t val = std::stoull(sval);
                                put(key, val);
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if (op == "get") {
						string key = next_op(args, i);
                        if (!key.empty()) {
                            std::cout << get(key) << std::endl;
                        } else {
                            return false;
                        }
                    } else if (op == "clear") {
                        clear();
                    } else if (op == "run") {
						string filename = next_op(args, i);
                        if (!filename.empty()) {
                            return run(filename, [&](vector<string> args) {
                                return exec(args);
                            });
                        } else {
                            return false;
                        }
                    } else if (op == "print") {
                        std::cout << print();
                    } else if (op == "help") {
                        help();
                    }
                } else {
                    return true;
                }
            }
        }
        return true;
	}

    void help() {
		std::cout << "usage: store vhash [OPERATIONS]" << std::endl
				  << "key type: string" << std::endl << std::endl
				  << "value type: uint64_t" << std::endl << std::endl
				  << "OPERATIONS:" << std::endl
        		  << "  put key data     Put (key, data) to the table" << std::endl
        		  << "  get key          Read data from the table given a key" << std::endl
        		  << "  repeat n         Repeat the next operation n times" << std::endl
        		  << "  run file         Run a script file" << std::endl
        		  << "  clear            Delete all elements" << std::endl
        		  << "  print            Print the entire list" << std::endl
        		  << "  help             Display help" << std::endl;
	}
};

void perform_vhash(vector<string> args) {
	vhash self;
	self.exec(args);
}