package {
    [Ruffle(InstanceAllocator)]
    [Ruffle(CallHandler)]
    public final class Namespace {
        public static const length = 2;

        // 13.2.2 The Namespace Constructor
        public function Namespace(prefixValue:* = undefined, uriValue:* = undefined) {
            this.init(arguments.length, prefixValue, uriValue);
        }

        private native function init(argCount: uint, prefixValue:*, uriValue:*): void;

        // 13.2.5.1 prefix
        public function get prefix(): String {
            return "";
        }
        public native function get uri(): String;

        AS3 function toString(): String {
            trace("// toString")
            return this.uri;
        }

        prototype.toString = function(): String {
            trace("// 1")
            trace(Object.prototype.toString.call(this));
            trace("// 2")
            var self: Namespace = this;
            return self.AS3::toString();
        }
        prototype.setPropertyIsEnumerable("toString", false);
    }
}