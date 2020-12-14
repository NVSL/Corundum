#include "inc/plist.hpp"
#include "inc/vlist.hpp"
#include "inc/pbst.hpp"
#include "inc/vbst.hpp"
#include "inc/phash.hpp"
#include "inc/vhash.hpp"
#include <vector>

using namespace std;

int main(int argc, char *argv[]) {
	if (argc < 2) {
		cerr << "usage: " << argv[0]
			 << " [vlist|plist|vbst|pbst|vhash|phash] [OPERATION]" << endl;
		return 1;
	}

	vector<string> args(argv, argv+argc);

	string tp = argv[1];

	if (tp == "plist") { perform_plist(args); }
	else if (tp == "vlist") { perform_vlist(args); }
	else if (tp == "pbst") { perform_pbst(args); }
	else if (tp == "vbst") { perform_vbst(args); }
	else if (tp == "phash") { perform_phash(args); }
	else if (tp == "vhash") { perform_vhash(args); }

	return 0;
}