(function() {var implementors = {};
implementors["num_bigint"] = [{"text":"impl LowerHex for BigInt","synthetic":false,"types":[]},{"text":"impl LowerHex for BigUint","synthetic":false,"types":[]}];
implementors["num_complex"] = [{"text":"impl&lt;T&gt; LowerHex for Complex&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: LowerHex + Num + PartialOrd + Clone,&nbsp;</span>","synthetic":false,"types":[]}];
implementors["num_rational"] = [{"text":"impl&lt;T:&nbsp;LowerHex + Clone + Integer&gt; LowerHex for Ratio&lt;T&gt;","synthetic":false,"types":[]}];
implementors["term_painter"] = [{"text":"impl&lt;T:&nbsp;LowerHex&gt; LowerHex for Painted&lt;T&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()